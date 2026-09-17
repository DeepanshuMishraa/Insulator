//! In-app iOS Simulator stream: lifecycle + Simulator tab handoff.
//!
//! T3 Code parity, thin slice: one owned `serve-sim --detach` stream for an
//! explicit UDID, shown under the right-panel Simulator tab. The tab owns a
//! dedicated `BrowserView` (separate from the Browser tab) that navigates to
//! the local preview URL. All subprocess I/O runs on `background_executor`;
//! `render` only reads the cached `sim_stream_*` fields.

use super::*;
use crate::sim_stream::{self, SimSupport, SimTheme};

impl Insulator {
    /// The Simulator tab's dedicated browser surface. Separate from the
    /// Browser tab's entity so the stream never clobbers a browsing session.
    /// Chromeless (no address bar): the stream is the content, not a page.
    fn ensure_sim_browser(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<crate::browser::BrowserView> {
        let browser_id = *self.sim_browser_id.get_or_insert_with(Uuid::new_v4);
        let browser = self.ensure_right_panel_browser(browser_id, window, cx);
        browser.update(cx, |view, cx| view.set_chromeless(cx));
        browser
    }

    /// The serve-sim letterbox color: this panel's own surface, so the page
    /// tracks the active app theme (including custom themes), not a hardcoded
    /// black/white.
    fn sim_page_background(cx: &App) -> String {
        let rgb = Theme::current(cx).surface.to_rgb();
        format!(
            "#{:02X}{:02X}{:02X}",
            (rgb.r * 255.0).round().clamp(0.0, 255.0) as u8,
            (rgb.g * 255.0).round().clamp(0.0, 255.0) as u8,
            (rgb.b * 255.0).round().clamp(0.0, 255.0) as u8,
        )
    }

    /// Navigate the Simulator tab's browser to the stream URL and reveal it.
    fn open_sim_url(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        let browser = self.ensure_sim_browser(window, cx);
        browser.update(cx, |view, cx| view.navigate_to_url(url, cx));
        self.right_panel_visible = true;
        self.right_panel_upper_tab = RightPanelUpperTab::Simulator;
        if let Some(browser_id) = self.sim_browser_id {
            self.right_panel_pending_browser_focus = Some(browser_id);
        }
        // The preview renders chrome asynchronously; strip on a schedule and
        // match the page background to the app theme immediately.
        let bg = Self::sim_page_background(cx);
        let dark = Theme::current(cx).is_dark;
        self.inject_sim_chrome(bg.clone(), dark, cx);
        self.schedule_kiosk_passes(bg, dark, cx);
        cx.notify();
    }

    fn sim_unsupported_message() -> Option<String> {
        match sim_stream::support() {
            SimSupport::Supported => None,
            SimSupport::UnsupportedPlatform => Some(
                crate::sim_stream::SimStreamError::UnsupportedPlatform.to_string(),
            ),
            SimSupport::UnsupportedArch => {
                Some(crate::sim_stream::SimStreamError::UnsupportedArch.to_string())
            }
        }
    }

    /// Start (or re-open) the owned simulator stream.
    ///
    /// `udid`: explicit device when known; `None` auto-picks the booted (or
    /// first available) device, boots it, then starts `serve-sim --detach`.
    /// The preview URL opens in the Simulator tab when it lands.
    pub(super) fn start_sim_stream(
        &mut self,
        udid: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(message) = Self::sim_unsupported_message() {
            self.show_toast(message);
            return;
        }
        if self.sim_stream_loading {
            return;
        }
        // Re-open a known stream without spawning a second helper.
        if udid.is_none() && self.sim_stream_info.is_some() {
            self.reopen_sim_stream(window, cx);
            return;
        }
        self.sim_stream_loading = true;
        self.sim_stream_error = None;
        // Reveal the tab immediately so the loader paints while we boot.
        self.right_panel_visible = true;
        self.right_panel_upper_tab = RightPanelUpperTab::Simulator;
        let dark = Theme::current(cx).is_dark;
        // Optimistic: `--theme` applies this polarity at start; a failure
        // clears it below so the next sync retries.
        self.sim_applied_dark = Some(dark);
        let theme = if dark { SimTheme::Dark } else { SimTheme::Light };
        let generation = self.sim_stream_generation.wrapping_add(1);
        self.sim_stream_generation = generation;
        cx.notify();
        let window_handle = window.window_handle();
        let entity = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let udid = match udid {
                        Some(udid) => {
                            sim_stream::validate_udid(&udid).map_err(|e| e.to_string())?;
                            udid
                        }
                        None => {
                            let devices =
                                sim_stream::list_devices_blocking().map_err(|e| e.to_string())?;
                            sim_stream::pick_preferred(&devices)
                                .map(|d| d.udid)
                                .ok_or_else(|| sim_stream::SimStreamError::NoDevices.to_string())?
                        }
                    };
                    // Reuse a live stream for the same device instead of
                    // spawning a second helper on another port.
                    if let Ok(streams) = sim_stream::list_streams_blocking(Some(&udid)) {
                        if let Some(existing) = streams.into_iter().next() {
                            return Ok(existing);
                        }
                    }
                    // Booting an already-booted device is a no-op.
                    let _ = sim_stream::boot_device_blocking(&udid);
                    sim_stream::start_detached_blocking(&udid, theme).map_err(|e| e.to_string())
                })
                .await;
            let _ = entity.update(cx, |this, cx| {
                // A Stop that landed while this start was in flight wins:
                // drop the stale result instead of resurrecting the view.
                if this.sim_stream_generation != generation {
                    return;
                }
                this.sim_stream_loading = false;
                match &result {
                    Ok(info) => {
                        this.sim_stream_info = Some(info.clone());
                        this.sim_stream_error = None;
                    }
                    Err(message) => {
                        this.sim_stream_error = Some(message.clone());
                        this.sim_applied_dark = None;
                        this.show_toast(message.clone());
                    }
                }
                cx.notify();
            });
            if let Ok(info) = result {
                let url = info.url.clone();
                let _ = window_handle.update(cx, |_, window, cx| {
                    let _ = entity.update(cx, |this, cx| {
                        // Stale start (Stop won the race): never re-open.
                        if this.sim_stream_generation != generation {
                            return;
                        }
                        this.open_sim_stream_url(url.clone(), window, cx);
                    });
                });
            }
        })
        .detach();
    }

    /// Poll the owned stream once and open it if still running (re-open after
    /// restart without spawning a second helper).
    pub(super) fn reopen_sim_stream(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(message) = Self::sim_unsupported_message() {
            self.show_toast(message);
            return;
        }
        if self.sim_stream_loading {
            return;
        }
        self.sim_stream_loading = true;
        self.right_panel_visible = true;
        self.right_panel_upper_tab = RightPanelUpperTab::Simulator;
        let generation = self.sim_stream_generation.wrapping_add(1);
        self.sim_stream_generation = generation;
        cx.notify();
        let window_handle = window.window_handle();
        let entity = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { sim_stream::list_streams_blocking(None) })
                .await;
            let url = entity
                .update(cx, |this, cx| {
                    if this.sim_stream_generation != generation {
                        return None;
                    }
                    this.sim_stream_loading = false;
                    match result {
                        Ok(mut streams) => {
                            streams.sort_by(|a, b| a.device.cmp(&b.device));
                            if let Some(info) = streams.into_iter().next() {
                                this.sim_stream_info = Some(info.clone());
                                this.sim_stream_error = None;
                                // Reattached after restart: appearance unknown,
                                // so sync it to the app theme unconditionally.
                                this.sim_applied_dark = None;
                                this.sync_sim_appearance(cx);
                                Some(info.url)
                            } else {
                                this.show_toast("no running simulator stream".to_owned());
                                None
                            }
                        }
                        Err(error) => {
                            let message = error.to_string();
                            this.sim_stream_error = Some(message.clone());
                            this.show_toast(message);
                            None
                        }
                    }
                })
                .unwrap_or(None);
            if let Some(url) = url {
                let _ = window_handle.update(cx, |_, window, cx| {
                    let _ = entity.update(cx, |this, cx| {
                        if this.sim_stream_generation != generation {
                            return;
                        }
                        this.open_sim_stream_url(url.clone(), window, cx);
                    });
                });
            }
        })
        .detach();
    }

    /// Open a landed stream URL in the Simulator tab. Reached via the
    /// captured window handle so background completion can hand off without
    /// holding a `Window`.
    pub(super) fn open_sim_stream_url(
        &mut self,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_sim_url(url, window, cx);
    }

    /// Full Stop: kill every stream helper and shut down every booted
    /// simulator. Deliberately wider than the owned UDID: after an abrupt
    /// quit + relaunch the owned state is gone but helpers and sims may
    /// still eat resources — Stop must always work, instantly and observably.
    ///
    /// The live view drops synchronously (the tab falls back to the stopping
    /// loader that same frame); only the subprocess cleanup waits. Any start
    /// still in flight is superseded via the generation counter.
    pub(super) fn stop_sim_stream(&mut self, cx: &mut Context<Self>) {
        // Already stopping: don't stack cleanups.
        if self.sim_stopping {
            return;
        }
        let generation = self.sim_stream_generation.wrapping_add(1);
        self.sim_stream_generation = generation;
        self.sim_stream_loading = true;
        // Drop the live view immediately so the tab falls back to the
        // stopping loader while the subprocess cleanup lands off-thread.
        self.sim_stream_info = None;
        self.sim_stopping = true;
        self.sim_applied_dark = None;
        // Park the webview on a blank page now so the dead preview can't
        // linger or flash if it renders another frame before cleanup lands.
        if let Some(browser_id) = self.sim_browser_id
            && let Some(browser) = self.right_panel_browsers.get(&browser_id)
        {
            browser.update(cx, |view, cx| {
                view.navigate_to_url("about:blank".to_owned(), cx);
            });
        }
        cx.notify();
        let entity = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { sim_stream::stop_all_blocking() })
                .await;
            let _ = entity.update(cx, |this, cx| {
                // A fresh Start supersedes this cleanup; leave its state alone.
                if this.sim_stream_generation != generation {
                    return;
                }
                this.sim_stream_loading = false;
                this.sim_stopping = false;
                match result {
                    Ok(()) => {
                        this.sim_stream_info = None;
                        this.sim_stream_error = None;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        this.sim_stream_error = Some(message.clone());
                        this.show_toast(message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Strip serve-sim chrome in the live view so only the simulator shows,
    /// on the app's surface background. Idempotent; re-run after load and
    /// theme flips because the React page renders chrome asynchronously.
    fn inject_sim_chrome(&self, bg: String, dark: bool, cx: &mut Context<Self>) {
        let Some(browser_id) = self.sim_browser_id else {
            return;
        };
        let Some(browser) = self.right_panel_browsers.get(&browser_id) else {
            return;
        };
        let script = sim_stream::kiosk_script_with_background(&bg, dark);
        browser.update(cx, |view, _| view.evaluate_page_script(&script));
    }

    /// Re-apply the kiosk CSS on a short schedule after navigation: one shot
    /// races the React page, three spaced passes do not. Stops early when
    /// the stream is gone.
    fn schedule_kiosk_passes(&self, bg: String, dark: bool, cx: &mut Context<Self>) {
        let generation = self.sim_stream_generation;
        let entity = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            for delay_ms in [1500u64, 3500, 7000] {
                cx.background_executor()
                    .timer(Duration::from_millis(delay_ms))
                    .await;
                let done = entity
                    .update(cx, |this, cx| {
                        if this.sim_stream_info.is_none()
                            || this.sim_stream_generation != generation
                        {
                            return true;
                        }
                        this.inject_sim_chrome(bg.clone(), dark, cx);
                        false
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    /// Mirror the app's light/dark polarity onto the live simulator without
    /// restarting the stream (`simctl ui appearance` is instant; the helper
    /// keeps streaming). No-op when no stream is live or polarity unchanged.
    pub(super) fn sync_sim_appearance(&mut self, cx: &mut Context<Self>) {
        let Some(info) = self.sim_stream_info.clone() else {
            return;
        };
        let dark = Theme::current(cx).is_dark;
        if self.sim_applied_dark == Some(dark) {
            return;
        }
        self.sim_applied_dark = Some(dark);
        cx.notify();
        let entity = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { sim_stream::set_appearance_blocking(&info.device, dark) })
                .await;
            let _ = entity.update(cx, |this, cx| {
                if let Err(error) = result {
                    // Revert the optimistic flag so the next sync retries.
                    this.sim_applied_dark = None;
                    this.show_toast(error.to_string());
                } else {
                    // Theme flips replace the page chrome asynchronously;
                    // re-strip on the new surface background.
                    let bg = Self::sim_page_background(cx);
                    this.inject_sim_chrome(bg, dark, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Refresh the device list off-thread for future picker UI. Frames read
    /// `sim_devices`; `None` means no scan has landed yet.
    #[allow(dead_code)]
    pub(super) fn refresh_sim_devices(&mut self, cx: &mut Context<Self>) {
        if self.sim_devices_loading {
            return;
        }
        self.sim_devices_loading = true;
        let generation = self.sim_devices_generation.wrapping_add(1);
        self.sim_devices_generation = generation;
        cx.notify();
        let entity = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let devices: Result<Vec<crate::sim_stream::SimDevice>, String> = cx
                .background_executor()
                .spawn(async move {
                    sim_stream::list_devices_blocking().map_err(|e| e.to_string())
                })
                .await;
            let _ = entity.update(cx, |this, cx| {
                if this.sim_devices_generation != generation {
                    return;
                }
                this.sim_devices_loading = false;
                match devices {
                    Ok(devices) => this.sim_devices = Some(devices),
                    Err(message) => this.show_toast(message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    #[allow(dead_code)]
    pub(super) fn sim_stream_status(&self) -> Option<crate::sim_stream::SimStreamInfo> {
        self.sim_stream_info.clone()
    }

    /// The Simulator tab body: start button, dot-matrix loader, or live view.
    pub(super) fn render_sim_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::current(cx);
        if self.sim_stream_info.is_some() {
            return self.render_sim_live(window, cx);
        }
        // Stopping hides the stream instantly; the loader holds the tab with
        // explicit copy until the helpers and sims are actually down.
        let stopping = self.sim_stopping;
        if self.sim_stream_loading || stopping {
            return div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(12.0))
                .child(dot_matrix_loader(theme.text_secondary, 18.0))
                .child(
                    div()
                        .text_size(sp(12.5))
                        .text_color(theme.text_secondary)
                        .child(if stopping {
                            "Stopping simulator…"
                        } else {
                            "Starting simulator…"
                        }),
                )
                .into_any_element();
        }
        let start_focus = self.transcript_control_focus("sim-start", cx);
        let error = self.sim_stream_error.clone();
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .p(px(24.0))
            .child(crate::ui::icon(
                "icons/laptop.svg",
                28.0,
                theme.text_tertiary,
            ))
            .child(
                div()
                    .text_size(sp(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child("iOS Simulator"),
            )
            .child(
                div()
                    .max_w(px(280.0))
                    .text_center()
                    .text_size(sp(12.5))
                    .line_height(sp(17.0))
                    .text_color(theme.text_tertiary)
                    .child("Stream a booted simulator here and watch the agent verify its work live."),
            )
            .when_some(error, |el, message| {
                el.child(
                    div()
                        .max_w(px(300.0))
                        .text_center()
                        .text_size(sp(12.0))
                        .text_color(theme.danger)
                        .child(message),
                )
            })
            .child(
                div()
                    .id("sim-start-button")
                    .track_focus(&start_focus)
                    .tab_index(0)
                    .mt(px(8.0))
                    .h(px(32.0))
                    .px(px(16.0))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .bg(theme.accent)
                    .text_size(sp(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.on_inverse)
                    .focus_visible(|style| {
                        style
                            .border_1()
                            .border_color(theme.accent)
                    })
                    .child("Start Simulator")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.start_sim_stream(None, window, cx);
                    }))
                    .on_key_down(cx.listener(
                        |this, event: &KeyDownEvent, window, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                this.start_sim_stream(None, window, cx);
                                cx.stop_propagation();
                            }
                        },
                    )),
            )
            .into_any_element()
    }

    /// Live stream: status header with Stop plus the dedicated browser view.
    fn render_sim_live(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::current(cx);
        let browser = self.ensure_sim_browser(window, cx);
        if let Some(sim_browser_id) = self.sim_browser_id {
            if self
                .right_panel_pending_browser_focus
                .take_if(|pending| *pending == sim_browser_id)
                .is_some()
            {
                browser.update(cx, |view, cx| view.focus_default(window, cx));
            }
        }
        let stop_focus = self.transcript_control_focus("sim-stop", cx);
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .py(px(6.0))
                    .child(div().size(px(8.0)).rounded_full().bg(theme.success))
                    .child(
                        div()
                            .text_size(sp(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_secondary)
                            .child("Simulator live"),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("sim-stop-button")
                            .track_focus(&stop_focus)
                            .tab_index(0)
                            .h(px(26.0))
                            .px(px(10.0))
                            .rounded(px(6.0))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .text_size(sp(12.0))
                            .text_color(theme.text_secondary)
                            .hover(|style| style.bg(theme.overlay).text_color(theme.text))
                            .active(|style| style.bg(theme.overlay_strong))
                            .focus_visible(|style| {
                                style.bg(theme.overlay).border_1().border_color(theme.accent)
                            })
                            .child("Stop")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stop_sim_stream(cx);
                            }))
                            .on_key_down(cx.listener(
                                |this, event: &KeyDownEvent, _, cx| {
                                    if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                    {
                                        this.stop_sim_stream(cx);
                                        cx.stop_propagation();
                                    }
                                },
                            )),
                    ),
            )
            .child(div().flex_1().min_h_0().child(browser))
            .into_any_element()
    }
}
