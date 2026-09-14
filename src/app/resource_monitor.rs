use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Div, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, ScrollHandle, Stateful, Styled, WeakEntity, div, px,
};

use crate::app::Insulator;
use crate::theme::{Theme, sp};
use crate::ui::icon;
use crate::ui::menu::{ContextMenuHandle, MenuAlign, popover};
use crate::ui::tooltip::Tooltip;

pub const RESOURCE_MONITOR_MENU_ID: &str = "resource-monitor-menu";

#[derive(Clone, Debug)]
pub struct ProcessResourceEntry {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub rss_bytes: u64,
    pub depth: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ResourceViewMode {
    #[default]
    Tree,
    Flat,
    Terminal,
}

#[derive(Clone, Debug, Default)]
pub struct ResourceUsageSnapshot {
    pub total_cpu_percent: f32,
    pub total_rss_bytes: u64,
    pub tree_entries: Vec<ProcessResourceEntry>,
    pub flat_entries: Vec<ProcessResourceEntry>,
    pub collected_at: Option<Instant>,
}

#[derive(Clone, Debug)]
struct RawProcess {
    pid: u32,
    ppid: u32,
    cpu: f32,
    rss_kb: u64,
    comm: String,
}

#[derive(Clone, Debug)]
struct Sample {
    pid: u32,
    ppid: u32,
    /// Cumulative CPU seconds from `ps TIME` (centisecond resolution on macOS).
    cpu_secs: Option<f64>,
    /// `ps %CPU` decaying average, used only when no delta baseline exists.
    cpu_fallback: f32,
    rss_kb: u64,
    comm: String,
}

/// One `ps` snapshot. `TIME` is cumulative per-process CPU time, which is what
/// lets the caller derive an instantaneous rate from two samples instead of
/// trusting `ps %CPU` (a decaying ~1-minute average that lags and smooths
/// spikes).
fn sample_processes() -> HashMap<u32, Sample> {
    let mut out = HashMap::new();
    let output = Command::new("ps")
        .args(["-eo", "pid,ppid,time,%cpu,rss,comm"])
        .output();
    let Ok(output) = output else {
        return out;
    };
    if !output.status.success() {
        return out;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("PID") {
            continue;
        }
        // pid, ppid, time, %cpu and rss contain no whitespace; everything
        // after them is the command path.
        let mut parts = trimmed.split_whitespace();
        let (Some(pid), Some(ppid), Some(time), Some(cpu), Some(rss_kb)) = (
            parts.next().and_then(|p| p.parse::<u32>().ok()),
            parts.next().and_then(|p| p.parse::<u32>().ok()),
            parts.next(),
            parts.next().and_then(|p| p.parse::<f32>().ok()),
            parts.next().and_then(|p| p.parse::<u64>().ok()),
        ) else {
            continue;
        };
        let comm = parts.collect::<Vec<_>>().join(" ");
        if comm.is_empty() {
            continue;
        }
        out.insert(
            pid,
            Sample {
                pid,
                ppid,
                cpu_secs: parse_ps_time(time),
                cpu_fallback: cpu,
                rss_kb,
                comm,
            },
        );
    }
    out
}

/// Parse BSD `ps TIME` (`MM:SS.cc` or `HH:MM:SS.cc`) into cumulative seconds.
fn parse_ps_time(field: &str) -> Option<f64> {
    let mut parts = field.split(':').collect::<Vec<_>>();
    let secs = parts.pop()?.parse::<f64>().ok()?;
    let mut total = secs;
    let mut factor = 60.0;
    // Remaining parts, right to left, are minutes then hours (days never
    // appear; minutes grow past 59 instead).
    while let Some(part) = parts.pop() {
        total += part.parse::<f64>().ok()? * factor;
        factor *= 60.0;
    }
    Some(total)
}

/// Start time of a process as unix seconds, used to attribute WebKit XPC
/// services (reparented to `launchd`, so the ppid walk cannot see them) to the
/// app instance that launched before them.
#[cfg(target_os = "macos")]
fn process_start_secs(pid: u32) -> Option<u64> {
    use std::mem::MaybeUninit;
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    // SAFETY: `proc_pidinfo` with `PROC_PIDTBSDINFO` writes exactly one
    // `proc_bsdinfo`; the byte count is checked before the value is read.
    let filled = unsafe {
        libc::proc_pidinfo(
            pid as libc::pid_t,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr() as *mut libc::c_void,
            std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int,
        )
    };
    if filled as usize != std::mem::size_of::<libc::proc_bsdinfo>() {
        return None;
    }
    let info = unsafe { info.assume_init() };
    let secs = info.pbi_start_tvsec;
    (secs != 0).then_some(secs)
}

#[cfg(not(target_os = "macos"))]
fn process_start_secs(_pid: u32) -> Option<u64> {
    None
}

fn is_webkit_xpc(comm: &str) -> bool {
    Path::new(comm)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|base| base.starts_with("com.apple.WebKit."))
}

pub fn collect_resource_usage(app_pid: u32, daemon_pid: Option<u32>) -> ResourceUsageSnapshot {
    // Two samples ~1s apart: per-process CPU% is the delta of cumulative CPU
    // time over wall time, i.e. what Activity Monitor shows, not the decaying
    // average `ps %CPU` reports. Runs on the background executor already.
    let wall_start = Instant::now();
    let first = sample_processes();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    let wall_secs = wall_start.elapsed().as_secs_f32().max(0.001);
    let second = sample_processes();

    let mut raw_procs = HashMap::new();
    let mut children_by_ppid: HashMap<u32, Vec<u32>> = HashMap::new();

    for sample in second.values() {
        let cpu = match (
            first.get(&sample.pid).and_then(|prev| prev.cpu_secs),
            sample.cpu_secs,
        ) {
            (Some(before), Some(after)) => {
                let delta = after - before;
                if delta < 0.0 {
                    // PID reuse across the interval; fall back to the average.
                    sample.cpu_fallback
                } else {
                    delta as f32 / wall_secs * 100.0
                }
            }
            // Process appeared between samples: no baseline, so the
            // since-birth average is the most honest instantaneous proxy.
            _ => sample.cpu_fallback,
        };
        children_by_ppid
            .entry(sample.ppid)
            .or_default()
            .push(sample.pid);
        raw_procs.insert(
            sample.pid,
            RawProcess {
                pid: sample.pid,
                ppid: sample.ppid,
                cpu,
                rss_kb: sample.rss_kb,
                comm: sample.comm.clone(),
            },
        );
    }

    // Determine daemon PID if not supplied
    let effective_daemon_pid = daemon_pid.or_else(|| {
        raw_procs.values().find_map(|p| {
            if (p.comm.contains("waku-daemon") || p.comm.contains("insulator-daemon"))
                && p.pid != app_pid
            {
                Some(p.pid)
            } else {
                None
            }
        })
    });

    let mut tree_entries = Vec::new();
    let mut visited = HashSet::new();

    // 1. App process and everything parented to it.
    if raw_procs.contains_key(&app_pid) {
        collect_subtree(
            app_pid,
            0,
            &raw_procs,
            &children_by_ppid,
            &mut tree_entries,
            &mut visited,
            effective_daemon_pid,
            app_pid,
            true,
        );
    }

    // 2. WebKit XPC services backing our WKWebViews. They are reparented to
    // `launchd` (ppid 1), so the tree walk above never reaches them even
    // though their CPU/RSS is ours — a WebContent process alone can outweigh
    // the app row. Attribute the ones launched at or after this app instance
    // started; anything older belongs to another app (e.g. Safari) and stays
    // out rather than inflating our totals.
    if let Some(app_start) = process_start_secs(app_pid) {
        let mut webkit: Vec<&RawProcess> = raw_procs
            .values()
            .filter(|p| {
                !visited.contains(&p.pid)
                    && is_webkit_xpc(&p.comm)
                    && process_start_secs(p.pid).is_some_and(|start| start >= app_start)
            })
            .collect();
        webkit.sort_by(|a, b| b.rss_kb.cmp(&a.rss_kb));
        // Nest directly under the App row so the flat view keeps working and
        // the tree shows exactly what is included.
        let insert_at = tree_entries
            .first()
            .filter(|entry| entry.pid == app_pid)
            .map(|_| 1)
            .unwrap_or(tree_entries.len());
        for (offset, proc_) in webkit.iter().enumerate() {
            visited.insert(proc_.pid);
            tree_entries.insert(
                insert_at + offset,
                ProcessResourceEntry {
                    pid: proc_.pid,
                    ppid: proc_.ppid,
                    name: clean_process_name(&proc_.comm),
                    cpu_percent: proc_.cpu,
                    rss_bytes: proc_.rss_kb * 1024,
                    depth: 1,
                },
            );
        }
    }

    // 3. Daemon process (Main) and its subprocesses
    if let Some(dpid) = effective_daemon_pid {
        if !visited.contains(&dpid) && raw_procs.contains_key(&dpid) {
            collect_subtree(
                dpid,
                0,
                &raw_procs,
                &children_by_ppid,
                &mut tree_entries,
                &mut visited,
                effective_daemon_pid,
                app_pid,
                false,
            );
        }
    }

    let mut total_cpu_percent = 0.0;
    let mut total_rss_bytes = 0;
    for entry in &tree_entries {
        total_cpu_percent += entry.cpu_percent;
        total_rss_bytes += entry.rss_bytes;
    }

    // Build flat entries sorted by memory descending
    let mut flat_entries = tree_entries.clone();
    for entry in &mut flat_entries {
        entry.depth = 0;
    }
    flat_entries.sort_by(|a, b| b.rss_bytes.cmp(&a.rss_bytes));

    ResourceUsageSnapshot {
        total_cpu_percent,
        total_rss_bytes,
        tree_entries,
        flat_entries,
        collected_at: Some(Instant::now()),
    }
}

fn collect_subtree(
    pid: u32,
    depth: usize,
    raw_procs: &HashMap<u32, RawProcess>,
    children_by_ppid: &HashMap<u32, Vec<u32>>,
    out: &mut Vec<ProcessResourceEntry>,
    visited: &mut HashSet<u32>,
    daemon_pid: Option<u32>,
    app_pid: u32,
    is_renderer: bool,
) {
    if !visited.insert(pid) {
        return;
    }

    if let Some(proc) = raw_procs.get(&pid) {
        let name = if is_renderer && pid == app_pid {
            "App".to_string()
        } else if Some(pid) == daemon_pid {
            "Main".to_string()
        } else {
            clean_process_name(&proc.comm)
        };

        out.push(ProcessResourceEntry {
            pid,
            ppid: proc.ppid,
            name,
            cpu_percent: proc.cpu,
            rss_bytes: proc.rss_kb * 1024,
            depth,
        });

        if let Some(children) = children_by_ppid.get(&pid) {
            let mut sorted_children = children.clone();
            sorted_children.sort_by(|&a, &b| {
                let mem_a = raw_procs.get(&a).map_or(0, |p| p.rss_kb);
                let mem_b = raw_procs.get(&b).map_or(0, |p| p.rss_kb);
                mem_b.cmp(&mem_a)
            });

            for child_pid in sorted_children {
                collect_subtree(
                    child_pid,
                    depth + 1,
                    raw_procs,
                    children_by_ppid,
                    out,
                    visited,
                    daemon_pid,
                    app_pid,
                    false,
                );
            }
        }
    }
}

fn clean_process_name(comm: &str) -> String {
    let path = Path::new(comm);
    let base = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| comm.to_string());

    if base.starts_with("insulator-sidecar") || base.starts_with("waku-sidecar") {
        "Sidecar".to_string()
    } else {
        base
    }
}

impl Insulator {
    pub(super) fn render_resource_usage_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let weak = cx.entity().downgrade();
        let handle =
            self.menu_handle_with(RESOURCE_MONITOR_MENU_ID, cx, move |open, _window, cx| {
                if open {
                    let _ = weak.update(cx, |this, cx| {
                        this.refresh_resource_usage(cx);
                    });
                }
            });

        let theme = Theme::current(cx);
        let is_open = handle.is_open();

        let trigger = div()
            .id("resource-monitor-button")
            .w(px(26.0))
            .h(px(26.0))
            .flex_none()
            .rounded(px(6.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_default()
            .hover(|element| element.bg(theme.overlay))
            .active(|element| element.bg(theme.overlay_strong))
            .when(is_open, |element| element.bg(theme.overlay_strong))
            .tooltip(Tooltip::text(tr!("resource_monitor.title")))
            .child(icon(
                "icons/cpu.svg",
                14.0,
                if is_open {
                    theme.text
                } else {
                    theme.text_tertiary
                },
            ));

        let snapshot = self.resource_usage_snapshot.clone();
        let view_mode = self.resource_view_mode;
        let show_all = self.resource_show_all;
        let copied = self.resource_usage_copied;
        let loading = self.resource_usage_loading && snapshot.is_none();
        let weak = cx.entity().downgrade();
        let scroll_handle = self.resource_scroll_handle.clone();

        popover(
            trigger,
            &handle,
            MenuAlign::BelowLeft,
            move |handle, _window, cx| {
                render_resource_monitor_panel(
                    handle,
                    snapshot.as_ref(),
                    view_mode,
                    show_all,
                    copied,
                    loading,
                    &scroll_handle,
                    weak.clone(),
                    cx,
                )
            },
        )
    }

    pub(super) fn refresh_resource_usage(&mut self, cx: &mut Context<Self>) {
        let app_pid = std::process::id();
        let daemon_pid = self.daemon.pid();
        let generation = self.resource_usage_generation.wrapping_add(1);
        self.resource_usage_generation = generation;
        self.resource_usage_loading = true;

        let weak = cx.entity().downgrade();
        cx.spawn(async move |_this, cx| {
            let snapshot = cx
                .background_executor()
                .spawn(async move { collect_resource_usage(app_pid, daemon_pid) })
                .await;

            let _ = weak.update(cx, |this, cx| {
                if this.resource_usage_generation != generation {
                    return;
                }
                this.resource_usage_loading = false;
                this.resource_usage_snapshot = Some(snapshot);
                cx.notify();

                // If popover is still open, schedule the next refresh in 2 seconds
                if this.resource_popover_open() {
                    this.schedule_next_resource_refresh(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn resource_popover_open(&self) -> bool {
        self.menus
            .borrow()
            .get(RESOURCE_MONITOR_MENU_ID)
            .map_or(false, ContextMenuHandle::is_open)
    }

    fn schedule_next_resource_refresh(&mut self, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            let _ = weak.update(cx, |this, cx| {
                if this.resource_popover_open() {
                    this.refresh_resource_usage(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn copy_resource_usage(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.resource_usage_snapshot else {
            return;
        };

        let mut text = format!(
            "RESOURCES\nCPU: {:.1}%  Memory: {:.1} MB\n\n{:<8} {:<32} {:>8} {:>12}\n",
            snapshot.total_cpu_percent,
            snapshot.total_rss_bytes as f64 / (1024.0 * 1024.0),
            "PID",
            "NAME",
            "CPU",
            "MEM"
        );

        let entries = if self.resource_view_mode == ResourceViewMode::Flat {
            &snapshot.flat_entries
        } else {
            &snapshot.tree_entries
        };

        for entry in entries {
            let indent = "  ".repeat(entry.depth);
            let name = format!("{}{}", indent, entry.name);
            let mem = format!("{:.1} MB", entry.rss_bytes as f64 / (1024.0 * 1024.0));
            let cpu = format!("{:.1}%", entry.cpu_percent);
            text.push_str(&format!(
                "{:<8} {:<32} {:>8} {:>12}\n",
                entry.pid, name, cpu, mem
            ));
        }

        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        self.resource_usage_copied = true;
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1500))
                .await;
            let _ = weak.update(cx, |this, cx| {
                this.resource_usage_copied = false;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

fn render_resource_monitor_panel(
    handle: &ContextMenuHandle,
    snapshot: Option<&ResourceUsageSnapshot>,
    view_mode: ResourceViewMode,
    show_all: bool,
    copied: bool,
    loading: bool,
    scroll_handle: &ScrollHandle,
    weak: WeakEntity<Insulator>,
    cx: &App,
) -> AnyElement {
    let theme = Theme::current(cx);

    let (total_cpu, total_mem_mb) = match snapshot {
        Some(s) => (
            s.total_cpu_percent,
            s.total_rss_bytes as f64 / (1024.0 * 1024.0),
        ),
        None => (0.0, 0.0),
    };

    let entries = match (snapshot, view_mode) {
        (Some(s), ResourceViewMode::Flat) => &s.flat_entries,
        (Some(s), _) => &s.tree_entries,
        (None, _) => &[][..],
    };

    let visible_entries: &[ProcessResourceEntry] = if show_all || entries.len() <= 10 {
        entries
    } else {
        &entries[..10]
    };

    let mut panel = div()
        .id("resource-monitor-popover")
        .track_focus(handle.focus_handle())
        .w(px(330.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.elevated)
        .shadow_lg()
        .flex()
        .flex_col()
        .overflow_hidden();

    // Top Header Section
    let header = div()
        .p(px(14.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(sp(11.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text_secondary)
                        .child(tr!("resource_monitor.title")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.0))
                        .child(render_view_icon_button(
                            "resource-view-terminal",
                            "icons/terminal-square.svg",
                            view_mode == ResourceViewMode::Terminal,
                            ResourceViewMode::Terminal,
                            weak.clone(),
                            &theme,
                        ))
                        .child(render_view_icon_button(
                            "resource-view-flat",
                            "icons/layers.svg",
                            view_mode == ResourceViewMode::Flat,
                            ResourceViewMode::Flat,
                            weak.clone(),
                            &theme,
                        ))
                        .child(render_view_icon_button(
                            "resource-view-tree",
                            "icons/git-branch.svg",
                            view_mode == ResourceViewMode::Tree,
                            ResourceViewMode::Tree,
                            weak.clone(),
                            &theme,
                        ))
                        .child(render_copy_icon_button(
                            "resource-copy-button",
                            copied,
                            weak.clone(),
                            &theme,
                        )),
                ),
        )
        .child(
            div()
                .text_size(sp(12.5))
                .flex()
                .items_center()
                .gap(px(5.0))
                .child(
                    div()
                        .text_color(theme.text_secondary)
                        .child(tr!("resource_monitor.cpu")),
                )
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child(format!("{:.1}%", total_cpu)),
                )
                .child(
                    div()
                        .ml(px(8.0))
                        .text_color(theme.text_secondary)
                        .child(tr!("resource_monitor.memory")),
                )
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child(format!("{:.1} MB", total_mem_mb)),
                ),
        );

    panel = panel.child(header);
    panel = panel.child(div().h(px(1.0)).w_full().bg(theme.border));

    if loading {
        panel = panel.child(
            div()
                .py(px(30.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(sp(12.0))
                .text_color(theme.text_tertiary)
                .child(tr!("common.checking")),
        );
        return panel.into_any_element();
    }

    if view_mode == ResourceViewMode::Terminal {
        let terminal_view = div()
            .id("resource-terminal-view")
            .p(px(12.0))
            .max_h(px(320.0))
            .overflow_y_scroll()
            .track_scroll(scroll_handle)
            .font_family(crate::md::render::active_mono_family())
            .text_size(sp(11.0))
            .line_height(sp(16.0))
            .text_color(theme.text)
            .children(visible_entries.iter().map(|entry| {
                let indent = "  ".repeat(entry.depth);
                let mem = format!("{:.1} MB", entry.rss_bytes as f64 / (1024.0 * 1024.0));
                let cpu = format!("{:.1}%", entry.cpu_percent);
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(format!("{}{}", indent, entry.name)),
                    )
                    .child(div().w(px(55.0)).text_right().child(cpu))
                    .child(div().w(px(70.0)).text_right().child(mem))
            }));
        panel = panel.child(terminal_view);
    } else {
        // Table Header
        let table_header = div()
            .h(px(26.0))
            .px(px(14.0))
            .flex()
            .items_center()
            .justify_between()
            .text_size(sp(11.5))
            .text_color(theme.text_secondary)
            .child(div().flex_1().min_w_0().child(tr!("resource_monitor.name")))
            .child(
                div()
                    .w(px(55.0))
                    .text_right()
                    .child(tr!("resource_monitor.cpu")),
            )
            .child(
                div()
                    .w(px(75.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(2.0))
                    .child(tr!("resource_monitor.memory"))
                    .child(icon("icons/chevron-down.svg", 10.0, theme.text_secondary)),
            );

        panel = panel.child(table_header);

        // Process rows list
        let rows_container = div()
            .id("resource-monitor-rows")
            .max_h(px(320.0))
            .overflow_y_scroll()
            .track_scroll(scroll_handle)
            .flex()
            .flex_col()
            .children(visible_entries.iter().map(|entry| {
                let indent_px = (entry.depth as f32) * 14.0;
                let mem_mb = entry.rss_bytes as f64 / (1024.0 * 1024.0);

                div()
                    .h(px(24.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(sp(12.0))
                    .hover(|element| element.bg(theme.overlay))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pl(px(indent_px))
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(theme.text)
                            .child(entry.name.clone()),
                    )
                    .child(
                        div()
                            .w(px(55.0))
                            .text_right()
                            .font_family(crate::md::render::active_mono_family())
                            .text_color(theme.text)
                            .child(format!("{:.1}%", entry.cpu_percent)),
                    )
                    .child(
                        div()
                            .w(px(75.0))
                            .text_right()
                            .font_family(crate::md::render::active_mono_family())
                            .text_color(theme.text_secondary)
                            .child(format!("{:.1} MB", mem_mb)),
                    )
            }));

        panel = panel.child(rows_container);
    }

    // Footer with Show less / Show more button
    if entries.len() > 10 {
        panel = panel.child(div().h(px(1.0)).w_full().bg(theme.border));
        let footer_button = div()
            .id("resource-monitor-toggle-more")
            .h(px(32.0))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_default()
            .text_size(sp(12.0))
            .text_color(theme.text_secondary)
            .hover(|element| element.bg(theme.overlay).text_color(theme.text))
            .child(if show_all {
                tr!("resource_monitor.show_less")
            } else {
                tr!("resource_monitor.show_more")
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                let _ = weak.update(cx, |this, cx| {
                    this.resource_show_all = !show_all;
                    cx.notify();
                });
            });

        panel = panel.child(footer_button);
    }

    panel.into_any_element()
}

fn render_view_icon_button(
    id: &'static str,
    icon_path: &'static str,
    active: bool,
    target_mode: ResourceViewMode,
    weak: WeakEntity<Insulator>,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(22.0))
        .h(px(22.0))
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_default()
        .when(active, |el| el.bg(theme.overlay_strong))
        .when(!active, |el| el.hover(|e| e.bg(theme.overlay)))
        .child(icon(
            icon_path,
            12.0,
            if active {
                theme.text
            } else {
                theme.text_tertiary
            },
        ))
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let _ = weak.update(cx, |this, cx| {
                this.resource_view_mode = target_mode;
                cx.notify();
            });
        })
}

fn render_copy_icon_button(
    id: &'static str,
    copied: bool,
    weak: WeakEntity<Insulator>,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(22.0))
        .h(px(22.0))
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_default()
        .hover(|el| el.bg(theme.overlay))
        .tooltip(Tooltip::text(if copied {
            tr!("resource_monitor.copied")
        } else {
            tr!("common.copy")
        }))
        .child(icon(
            if copied {
                "icons/check.svg"
            } else {
                "icons/copy.svg"
            },
            12.0,
            if copied {
                theme.text
            } else {
                theme.text_tertiary
            },
        ))
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let _ = weak.update(cx, |this, cx| {
                this.copy_resource_usage(cx);
            });
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_time_parses_minutes_and_hours_with_centiseconds() {
        assert_eq!(parse_ps_time("0:00.02"), Some(0.02));
        assert_eq!(parse_ps_time("22:12.43"), Some(22.0 * 60.0 + 12.43));
        assert_eq!(parse_ps_time("1:02:03"), Some(3723.0));
        assert_eq!(parse_ps_time("43:02.72"), Some(43.0 * 60.0 + 2.72));
    }

    #[test]
    fn ps_time_rejects_garbage() {
        assert_eq!(parse_ps_time(""), None);
        assert_eq!(parse_ps_time("-"), None);
        assert_eq!(parse_ps_time("abc"), None);
        assert_eq!(parse_ps_time("12:ab"), None);
    }

    #[test]
    fn webkit_xpc_matches_service_binaries_only() {
        assert!(is_webkit_xpc(
            "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent"
        ));
        assert!(is_webkit_xpc(
            "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.GPU.xpc/Contents/MacOS/com.apple.WebKit.GPU"
        ));
        assert!(!is_webkit_xpc("/Applications/Safari.app/Contents/MacOS/Safari"));
        assert!(!is_webkit_xpc(
            "/Applications/Insulator.app/Contents/MacOS/Insulator"
        ));
    }
}
