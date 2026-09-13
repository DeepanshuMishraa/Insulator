use gpui::{Hsla, Window};
use std::path::PathBuf;

pub fn local_user_github_avatar_path() -> Option<PathBuf> {
    let path = dirs::home_dir()?
        .join(".insulator")
        .join("cache")
        .join("github-avatar.png");
    path.is_file().then_some(path)
}

pub fn set_font_smoothing_enabled(enabled: bool) {
    #[cfg(target_os = "macos")]
    {
        let bundle_id = if cfg!(debug_assertions) {
            "sh.insulator.dev"
        } else {
            "sh.insulator"
        };
        let value = if enabled { "1" } else { "0" };
        let _ = std::process::Command::new("defaults")
            .args(["write", bundle_id, "AppleFontSmoothing", "-int", value])
            .status();
    }
}

fn macos_picture_path(output: &[u8]) -> Option<PathBuf> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != "Picture:")
        .map(PathBuf::from)
}

fn macos_jpeg_photo(output: &[u8]) -> Option<Vec<u8>> {
    let hex = String::from_utf8_lossy(output)
        .split_whitespace()
        .skip_while(|token| *token != "JPEGPhoto:")
        .skip(1)
        .collect::<String>();
    if hex.is_empty() || hex.len() % 2 != 0 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok())
        .collect()
}

pub fn ensure_user_login_avatar_cached() {
    #[cfg(target_os = "macos")]
    {
        let Some(home) = dirs::home_dir() else { return };
        let cache_dir = home.join(".insulator").join("cache");
        let Some(user) = std::env::var_os("USER") else {
            return;
        };
        if std::fs::create_dir_all(&cache_dir).is_err() {
            return;
        }
        let jpeg_output = std::process::Command::new("dscl")
            .args([".", "-read"])
            .arg(format!("/Users/{}", user.to_string_lossy()))
            .arg("JPEGPhoto")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| macos_jpeg_photo(&output.stdout));
        if let Some(bytes) = jpeg_output {
            let target = cache_dir.join("login-avatar.jpg");
            if !target.is_file() {
                let temporary = cache_dir.join("login-avatar.tmp.jpg");
                if std::fs::write(&temporary, bytes).is_ok() {
                    let _ = std::fs::rename(temporary, target);
                }
            }
            return;
        }

        let Some(source) = std::process::Command::new("dscl")
            .args([".", "-read"])
            .arg(format!("/Users/{}", user.to_string_lossy()))
            .arg("Picture")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| macos_picture_path(&output.stdout))
            .filter(|source| source.is_file())
        else {
            return;
        };
        let target = cache_dir.join("login-avatar.png");
        if !target.is_file() {
            let temporary = cache_dir.join("login-avatar.tmp.png");
            let converted = std::process::Command::new("sips")
                .args(["-s", "format", "png"])
                .arg(source)
                .arg("--out")
                .arg(&temporary)
                .output()
                .is_ok_and(|output| output.status.success());
            if converted {
                let _ = std::fs::rename(temporary, target);
            } else {
                let _ = std::fs::remove_file(temporary);
            }
        }
    }
}

pub fn ensure_user_github_avatar_cached() {
    let Some(home) = dirs::home_dir() else { return };
    let cache_dir = home.join(".insulator").join("cache");
    let target = cache_dir.join("github-avatar.png");
    if target.exists() {
        return;
    }
    std::thread::spawn(move || {
        let _ = std::fs::create_dir_all(&cache_dir);
        if let Ok(output) = std::process::Command::new("gh")
            .args(["api", "user", "--jq", ".avatar_url"])
            .output()
        {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if url.starts_with("https://") {
                let _ = std::process::Command::new("curl")
                    .args(["-s", "-L", &url, "-o", &target.display().to_string()])
                    .output();
            }
        }
    });
}

/// Returns the unique font family names available to the current user.
/// Discovery is intentionally explicit and should run off the UI thread.
#[cfg(target_os = "macos")]
pub fn installed_font_families() -> Vec<String> {
    use std::ffi::{c_char, c_void};

    type CFArray = c_void;
    type CFString = c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(value: *const c_void);
        fn CFArrayGetCount(array: *const CFArray) -> isize;
        fn CFArrayGetValueAtIndex(array: *const CFArray, index: isize) -> *const c_void;
        fn CFStringGetCString(
            string: *const CFString,
            buffer: *mut c_char,
            buffer_size: isize,
            encoding: u32,
        ) -> bool;
    }
    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontManagerCopyAvailableFontFamilyNames() -> *const CFArray;
    }

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    let array = unsafe { CTFontManagerCopyAvailableFontFamilyNames() };
    if array.is_null() {
        return Vec::new();
    }

    let mut families = Vec::new();
    let count = unsafe { CFArrayGetCount(array) };
    for index in 0..count {
        let value = unsafe { CFArrayGetValueAtIndex(array, index) };
        if value.is_null() {
            continue;
        }
        let mut buffer = [0 as c_char; 512];
        let converted = unsafe {
            CFStringGetCString(
                value.cast(),
                buffer.as_mut_ptr(),
                buffer.len() as isize,
                K_CF_STRING_ENCODING_UTF8,
            )
        };
        if converted {
            let bytes = buffer
                .iter()
                .take_while(|byte| **byte != 0)
                .map(|byte| *byte as u8)
                .collect::<Vec<_>>();
            if let Ok(family) = String::from_utf8(bytes) {
                families.push(family);
            }
        }
    }
    unsafe { CFRelease(array.cast()) };
    families.sort_unstable_by_key(|family| family.to_lowercase());
    families.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    families
}

#[cfg(not(target_os = "macos"))]
pub fn installed_font_families() -> Vec<String> {
    Vec::new()
}

#[cfg(target_os = "macos")]
pub fn show_about_panel() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    NSApplication::sharedApplication(main_thread).orderFrontStandardAboutPanel(None);
}

#[cfg(not(target_os = "macos"))]
pub fn show_about_panel() {}

/// Register embedded font data with CoreText at process scope. GPUI's
/// `add_fonts` only feeds its private font-kit source, which CoreText cascade
/// matching cannot see — and it refuses symbols-only faces outright (fonts
/// with no 'm' glyph). Fonts referenced through `FontFallbacks` therefore
/// must be registered here instead.
#[cfg(target_os = "macos")]
pub fn register_fonts_with_coretext(fonts: &[&'static [u8]]) -> anyhow::Result<()> {
    use std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGDataProviderCreateWithData(
            info: *mut c_void,
            data: *const u8,
            size: usize,
            release_callback: *const c_void,
        ) -> *mut c_void;
        fn CGFontCreateWithDataProvider(provider: *mut c_void) -> *mut c_void;
        fn CGDataProviderRelease(provider: *mut c_void);
        fn CGFontRelease(font: *mut c_void);
    }
    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontManagerRegisterGraphicsFont(font: *mut c_void, error: *mut *mut c_void) -> bool;
    }

    for (index, data) in fonts.iter().enumerate() {
        unsafe {
            let provider = CGDataProviderCreateWithData(
                std::ptr::null_mut(),
                data.as_ptr(),
                data.len(),
                std::ptr::null(),
            );
            anyhow::ensure!(!provider.is_null(), "font {index}: not a readable buffer");
            let font = CGFontCreateWithDataProvider(provider);
            CGDataProviderRelease(provider);
            anyhow::ensure!(!font.is_null(), "font {index}: not a valid font");
            let registered = CTFontManagerRegisterGraphicsFont(font, std::ptr::null_mut());
            CGFontRelease(font);
            anyhow::ensure!(registered, "font {index}: CoreText registration failed");
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn register_fonts_with_coretext(_: &[&'static [u8]]) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn init_reduce_motion(cx: &mut gpui::App) {
    use objc2_app_kit::NSWorkspace;

    cx.set_reduce_motion(NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion());
}

#[cfg(target_os = "linux")]
pub fn init_reduce_motion(cx: &mut gpui::App) {
    if let Ok(value) = std::env::var("INSULATOR_REDUCE_MOTION")
        && let Some(enabled) = parse_boolean_setting(&value)
    {
        cx.set_reduce_motion(enabled);
        return;
    }

    // GNOME exposes its animation preference through GSettings. Resolve it
    // once off the UI thread; frames only read GPUI's in-memory flag.
    cx.spawn(async move |cx| {
        let enabled = cx
            .background_executor()
            .spawn(async move { linux_reduce_motion_enabled() })
            .await;
        cx.update(|cx| cx.set_reduce_motion(enabled));
    })
    .detach();
}

#[cfg(target_os = "linux")]
fn linux_reduce_motion_enabled() -> bool {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| parse_boolean_setting(&value))
        .is_some_and(|animations_enabled| !animations_enabled)
}

/// Ease of Access → "Show animations in Windows" clears
/// `SPI_GETCLIENTAREAANIMATION`. GPUI has no Windows implementation of its
/// own, and the call only reads a cached user setting, so startup can ask
/// directly.
#[cfg(target_os = "windows")]
pub fn init_reduce_motion(cx: &mut gpui::App) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };

    let mut animations_enabled: i32 = 1;
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            std::ptr::from_mut(&mut animations_enabled).cast(),
            0,
        )
    };
    if read != 0 {
        cx.set_reduce_motion(animations_enabled == 0);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn init_reduce_motion(_: &mut gpui::App) {}

#[cfg(target_os = "linux")]
fn parse_boolean_setting(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Deliver an audible macOS notification. GPUI owns the notification-center
/// delegate (and therefore click responses); Insulator only supplies content here
/// because GPUI's generic payload does not currently expose a sound field.
#[cfg(target_os = "macos")]
pub fn show_task_notification(tag: &str, title: &str, body: &str, _: &gpui::App) {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSBundle, NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
        UNNotificationSound, UNUserNotificationCenter,
    };

    // UserNotifications raises an Objective-C exception for an executable
    // outside an application bundle, including unit tests and `cargo run`.
    if NSBundle::mainBundle().bundleIdentifier().is_none() {
        return;
    }

    let tag = tag.to_owned();
    let title = title.to_owned();
    let body = body.to_owned();
    let authorization = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
        if !granted.as_bool() {
            return;
        }

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&title));
        content.setBody(&NSString::from_str(&body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));

        // A nil trigger delivers immediately. The stable task tag replaces an
        // older completion banner for the same task and comes back on click.
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&tag),
            &content,
            None,
        );
        UNUserNotificationCenter::currentNotificationCenter()
            .addNotificationRequest_withCompletionHandler(&request, None);
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &authorization,
        );
}

#[cfg(not(target_os = "macos"))]
pub fn show_task_notification(tag: &str, title: &str, body: &str, cx: &gpui::App) {
    cx.show_system_notification(gpui::SystemNotification {
        tag: tag.to_owned().into(),
        title: title.to_owned().into(),
        body: body.to_owned().into(),
        actions: Vec::new(),
    });
}

#[cfg(target_os = "macos")]
fn app_icon_for_application_path(
    application_path: &objc2_foundation::NSString,
) -> Option<std::sync::Arc<gpui::Image>> {
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize};

    let image = NSWorkspace::sharedWorkspace().iconForFile(application_path);
    image.setSize(NSSize::new(32.0, 32.0));
    // Extract one small representation. `TIFFRepresentation` would serialize
    // the icon's entire rep stack — ~72 MB and hundreds of milliseconds per
    // app for a 1024px icon — and then hand GPUI a 1024px PNG to decode on
    // first paint. Proposing a 32pt rect selects the nearest small rep.
    let mut proposed = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(32.0, 32.0));
    let cg_image =
        unsafe { image.CGImageForProposedRect_context_hints(&mut proposed, None, None) }?;
    let bitmap_rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg_image);
    let properties = NSDictionary::new();
    let png_data = unsafe {
        bitmap_rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
    }?;
    let bytes = unsafe { png_data.as_bytes_unchecked() };
    (!bytes.is_empty()).then(|| {
        std::sync::Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            bytes.to_vec(),
        ))
    })
}

#[cfg(target_os = "macos")]
pub fn load_app_icon_for_bundle_id(bundle_id: &str) -> Option<std::sync::Arc<gpui::Image>> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    let bundle_id = NSString::from_str(bundle_id);
    let application_url =
        NSWorkspace::sharedWorkspace().URLForApplicationWithBundleIdentifier(&bundle_id)?;
    let application_path = application_url.path()?;
    app_icon_for_application_path(&application_path)
}

#[cfg(not(target_os = "macos"))]
pub fn load_app_icon_for_bundle_id(_: &str) -> Option<std::sync::Arc<gpui::Image>> {
    None
}

/// A folder-capable application the header's "open project in" control can
/// target, resolved against what is installed on this machine.
#[derive(Clone)]
pub struct ExternalApp {
    /// Stable identifier persisted as the user's preferred target.
    pub id: &'static str,
    pub label: &'static str,
    /// The bundle id that resolved here, for launching.
    pub bundle_id: &'static str,
    pub icon: std::sync::Arc<gpui::Image>,
}

/// Known folder-capable apps in menu order — editors, the file manager,
/// terminals, IDEs. An entry lists every bundle id it ships under; the first
/// installed one wins.
#[cfg(target_os = "macos")]
const TERMY_BUNDLE_ID: &str = "com.lassevestergaard.termy";

#[cfg(target_os = "macos")]
const OPEN_IN_CATALOG: &[(&str, &str, &[&str])] = &[
    ("vscode", "VS Code", &["com.microsoft.VSCode"]),
    ("cursor", "Cursor", &["com.todesktop.230313mzl4w4u92"]),
    ("zed", "Zed", &["dev.zed.Zed", "dev.zed.Zed-Preview"]),
    ("finder", "Finder", &["com.apple.finder"]),
    ("terminal", "Terminal", &["com.apple.Terminal"]),
    ("termy", "Termy", &[TERMY_BUNDLE_ID]),
    ("iterm2", "iTerm2", &["com.googlecode.iterm2"]),
    ("kitty", "Kitty", &["net.kovidgoyal.kitty"]),
    ("ghostty", "Ghostty", &["com.mitchellh.ghostty"]),
    ("warp", "Warp", &["dev.warp.Warp-Stable", "dev.warp.Warp"]),
    ("xcode", "Xcode", &["com.apple.dt.Xcode"]),
    (
        "android-studio",
        "Android Studio",
        &["com.google.android.studio"],
    ),
];

/// Resolve which catalog apps are installed, with their icons.
#[cfg(target_os = "macos")]
pub fn detect_open_in_apps() -> Vec<ExternalApp> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    let workspace = NSWorkspace::sharedWorkspace();
    OPEN_IN_CATALOG
        .iter()
        .filter_map(|&(id, label, bundle_ids)| {
            bundle_ids.iter().find_map(|&bundle_id| {
                let application_url = workspace
                    .URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))?;
                let application_path = application_url.path()?;
                Some(ExternalApp {
                    id,
                    label,
                    bundle_id,
                    icon: app_icon_for_application_path(&application_path)?,
                })
            })
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub fn detect_open_in_apps() -> Vec<ExternalApp> {
    Vec::new()
}

/// Open `path` in the application `bundle_id`, activating it. Launch Services
/// delivers the open asynchronously, so this never blocks.
#[cfg(target_os = "macos")]
pub fn open_path_in_app(path: &std::path::Path, bundle_id: &str) {
    use objc2_app_kit::{NSWorkspace, NSWorkspaceOpenConfiguration};
    use objc2_foundation::{NSArray, NSString, NSURL};

    let workspace = NSWorkspace::sharedWorkspace();
    let Some(application_url) =
        workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))
    else {
        return;
    };
    let url = if bundle_id == TERMY_BUNDLE_ID {
        // Termy rejects folder file URLs; its public new-tab route accepts the
        // working directory as an encoded query parameter instead.
        let Some(url) = NSURL::URLWithString(&NSString::from_str(&termy_open_url(path))) else {
            return;
        };
        url
    } else {
        NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
    };
    workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
        &NSArray::from_retained_slice(&[url]),
        &application_url,
        &NSWorkspaceOpenConfiguration::configuration(),
        None,
    );
}

#[cfg(target_os = "macos")]
fn termy_open_url(path: &std::path::Path) -> String {
    let mut url = url::Url::parse("termy://new").expect("static Termy URL should be valid");
    url.query_pairs_mut()
        .append_pair("dir", &path.to_string_lossy());
    url.into()
}

#[cfg(not(target_os = "macos"))]
pub fn open_path_in_app(_: &std::path::Path, _: &str) {}

/// Select `path` in the platform file manager. GPUI dispatches Linux portal
/// and subprocess work away from the UI thread.
pub fn reveal_in_file_manager(path: &std::path::Path, cx: &gpui::App) {
    cx.reveal_path(path);
}

/// Open `path` with its default application — a document in its editor.
pub fn open_with_default_app(path: &std::path::Path, cx: &gpui::App) {
    cx.open_with_system(path);
}

/// Decode the embedded desktop icon once. X11 consumes the RGBA pixels from
/// `WindowOptions`; Wayland associates the window through `app_id` and its
/// installed desktop entry.
#[cfg(target_os = "linux")]
pub fn linux_app_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
    static ICON: std::sync::LazyLock<Option<std::sync::Arc<image::RgbaImage>>> =
        std::sync::LazyLock::new(|| {
            image::load_from_memory(include_bytes!("../website/public/app-icon.png"))
                .ok()
                .map(|image| std::sync::Arc::new(image.into_rgba8()))
        });
    ICON.clone()
}

/// A compact shortcut label for the platform's primary GUI modifier.
pub const fn primary_shortcut<'a>(macos: &'a str, other: &'a str) -> &'a str {
    if cfg!(target_os = "macos") {
        macos
    } else {
        other
    }
}

/// Keep Insulator's single main window alive when the user closes it. This preserves
/// the current session and lets a Dock activation reveal the same GPUI window.
#[cfg(target_os = "macos")]
pub fn configure_main_window_close_behavior(window: &Window, cx: &gpui::App) {
    window.on_window_should_close(cx, |window, _| {
        hide_window(window);
        false
    });
}

#[cfg(not(target_os = "macos"))]
pub fn configure_main_window_close_behavior(_: &Window, _: &gpui::App) {}

#[cfg(target_os = "macos")]
pub fn hide_window(window: &mut Window) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    let Some(_main_thread) = MainThreadMarker::new() else {
        return;
    };

    // GPUI owns this view and its NSWindow. AppKit access stays on the main
    // thread, and orderOut hides without triggering GPUI's close callback.
    unsafe {
        let view = handle.ns_view.cast::<NSView>().as_ref();
        if let Some(native_window) = view.window() {
            native_window.orderOut(None);
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn hide_window(window: &mut Window) {
    window.remove_window();
}

#[cfg(target_os = "macos")]
thread_local! {
    static SIDEBAR_TINT_VIEW: std::cell::RefCell<Option<objc2::rc::Retained<objc2_app_kit::NSView>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_os = "macos")]
const SIDEBAR_WIDTH: f64 = 252.0;

pub fn start_window_move(window: &Window) {
    window.start_window_move();
}

/// Perform the platform's titlebar double-click action. GPUI delegates this
/// to the user's system preference on macOS, while Linux client decorations
/// must toggle maximize explicitly.
pub fn titlebar_double_click(window: &Window) {
    #[cfg(target_os = "macos")]
    window.titlebar_double_click();

    // Windows performs the user's configured caption double-click action in
    // `DefWindowProc`, which sees the click because the drag region reports
    // itself as caption to the hit test.
    #[cfg(target_os = "windows")]
    let _ = window;

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    if window.window_controls().maximize && window.is_resizable() {
        window.zoom_window();
    }
}

/// Match Cursor's macOS glass window stack without asking GPUI's transparent
/// Metal target to blend two translucent quads. The semantic tint is a native
/// view above active Sidebar vibrancy; GPUI paints clear sidebar chrome and one
/// translucent interaction layer above it.
#[cfg(target_os = "macos")]
pub fn configure_sidebar_material(
    window: &Window,
    dark: bool,
    sidebar_color: Hsla,
    sidebar_transparency: f32,
) {
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSColor, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };

    // GPUI owns the view hierarchy and creates the effect view before the
    // root entity is installed. We only adjust public AppKit properties.
    unsafe {
        let view = handle.ns_view.cast::<NSView>().as_ref();
        let Some(native_window) = view.window() else {
            return;
        };
        let background = if dark {
            NSColor::colorWithSRGBRed_green_blue_alpha(0.0, 0.0, 0.0, 0.25)
        } else {
            NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 1.0, 1.0, 0.0)
        };
        native_window.setBackgroundColor(Some(&background));

        let Some(content_view) = native_window.contentView() else {
            return;
        };

        let window_style = CURRENT_WINDOW_STYLE.get();
        if window_style != insulator_protocol::theme::WindowStyle::Solid
            && window_style != insulator_protocol::theme::WindowStyle::Transparent
        {
            SIDEBAR_TINT_VIEW.with_borrow(|slot| {
                if let Some(tint_view) = slot.as_ref() {
                    tint_view.setHidden(true);
                }
            });
            return;
        }

        let mut configured_effect = false;
        for subview in content_view.subviews().iter() {
            let Some(effect_view) = subview.downcast_ref::<NSVisualEffectView>() else {
                continue;
            };
            effect_view.setHidden(false);
            // Transparent mode keeps native vibrancy at full strength. The
            // slider changes the translucent app tint, not the blur itself.
            effect_view.setAlphaValue(1.0);
            effect_view.setMaterial(NSVisualEffectMaterial::Sidebar);
            effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            effect_view.setState(NSVisualEffectState::Active);
            configured_effect = true;
        }
        if !configured_effect {
            return;
        }

        // A style switch back from Transparent must restore the effect view;
        // otherwise its previous slider value leaves the sidebar invisible.
        for subview in content_view.subviews().iter() {
            if let Some(effect_view) = subview.downcast_ref::<NSVisualEffectView>() {
                effect_view.setAlphaValue(1.0);
            }
        }

        // One native material covers the whole window in Transparent mode.
        // A second sidebar-only tint makes the two regions look like they have
        // different blur strengths, so leave that extra layer hidden.
        if window_style == insulator_protocol::theme::WindowStyle::Transparent {
            SIDEBAR_TINT_VIEW.with_borrow(|slot| {
                if let Some(tint_view) = slot.as_ref() {
                    tint_view.setHidden(true);
                }
            });
            return;
        }

        let color = sidebar_color.to_rgb();
        let tint = NSColor::colorWithSRGBRed_green_blue_alpha(
            f64::from(color.r),
            f64::from(color.g),
            f64::from(color.b),
            f64::from(1.0 - sidebar_transparency.clamp(0.0, 100.0) / 100.0),
        );

        SIDEBAR_TINT_VIEW.with_borrow_mut(|slot| {
            let needs_new_view = slot.as_ref().is_none_or(|tint_view| {
                tint_view
                    .window()
                    .as_deref()
                    .is_none_or(|window| !std::ptr::eq(window, native_window.as_ref()))
            });
            if needs_new_view {
                let mut frame = content_view.bounds();
                frame.size.width = SIDEBAR_WIDTH;
                let tint_view = NSView::initWithFrame(NSView::alloc(main_thread), frame);
                tint_view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewHeightSizable);
                tint_view.setWantsLayer(true);
                content_view.addSubview_positioned_relativeTo(
                    &tint_view,
                    NSWindowOrderingMode::Below,
                    Some(view),
                );
                *slot = Some(tint_view);
            }

            if let Some(tint_view) = slot.as_ref() {
                tint_view.setHidden(false);
                if let Some(layer) = tint_view.layer() {
                    layer.setBackgroundColor(Some(&tint.CGColor()));
                }
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
pub fn configure_sidebar_material(_: &Window, _: bool, _: Hsla, _: f32) {}

#[cfg(target_os = "macos")]
pub fn set_sidebar_material_transparency(sidebar_color: Hsla, sidebar_transparency: f32) {
    use objc2_app_kit::NSColor;

    let transparency = if sidebar_transparency.is_finite() {
        sidebar_transparency.clamp(0.0, 100.0)
    } else {
        insulator_client::persistence::DEFAULT_SIDEBAR_TRANSPARENCY
    };
    let color = sidebar_color.to_rgb();
    let alpha = if transparency.is_finite() {
        1.0 - transparency.clamp(0.0, 100.0) / 100.0
    } else {
        1.0 - insulator_client::persistence::DEFAULT_SIDEBAR_TRANSPARENCY / 100.0
    };
    let tint = NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from(color.r),
        f64::from(color.g),
        f64::from(color.b),
        f64::from(alpha),
    );

    SIDEBAR_TINT_VIEW.with_borrow(|slot| {
        if let Some(tint_view) = slot.as_ref()
            && let Some(layer) = tint_view.layer()
        {
            layer.setBackgroundColor(Some(&tint.CGColor()));
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn set_sidebar_material_transparency(_: Hsla, _: f32) {}

thread_local! {
    static LIQUID_GLASS_VIEW: std::cell::RefCell<Option<objc2::rc::Retained<objc2_app_kit::NSView>>> =
        const { std::cell::RefCell::new(None) };
    static CURRENT_WINDOW_STYLE: std::cell::Cell<insulator_protocol::theme::WindowStyle> =
        const { std::cell::Cell::new(insulator_protocol::theme::WindowStyle::Solid) };
    static MAIN_WINDOW: std::cell::RefCell<Option<objc2::rc::Retained<objc2_app_kit::NSWindow>>> =
        const { std::cell::RefCell::new(None) };
    static MAIN_VIEW: std::cell::RefCell<Option<objc2::rc::Retained<objc2_app_kit::NSView>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_os = "macos")]
pub fn supports_liquid_glass() -> bool {
    use objc2::runtime::AnyClass;
    use objc2_foundation::NSProcessInfo;
    NSProcessInfo::processInfo()
        .operatingSystemVersion()
        .majorVersion
        >= 26
        || AnyClass::get(c"NSGlassView").is_some()
}

#[cfg(not(target_os = "macos"))]
pub fn supports_liquid_glass() -> bool {
    false
}

/// Relaunch the current executable with the same arguments, preserving the
/// persisted settings before the current process exits.
#[allow(dead_code)]
pub fn restart_application() -> bool {
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    std::process::Command::new(executable)
        .args(std::env::args_os().skip(1))
        .spawn()
        .is_ok()
}

#[cfg(target_os = "macos")]
unsafe fn apply_native_window_style(
    native_window: &objc2_app_kit::NSWindow,
    view: &objc2_app_kit::NSView,
    style: insulator_protocol::theme::WindowStyle,
    color_theme: insulator_protocol::theme::ColorTheme,
    main_thread: objc2::MainThreadMarker,
) {
    use objc2::MainThreadOnly;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSColor, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
    };

    let Some(content_view) = native_window.contentView() else {
        return;
    };
    let active_theme = crate::theme::Theme::from_color_theme(color_theme);
    let rgb = active_theme.surface.to_rgb();

    match style {
        insulator_protocol::theme::WindowStyle::LiquidGlass => {
            native_window.setOpaque(false);
            native_window.setHasShadow(true);
            native_window.setBackgroundColor(Some(&NSColor::clearColor()));

            // In Liquid Glass mode, hide the sidebar tint view so glass extends across the entire window
            SIDEBAR_TINT_VIEW.with_borrow(|slot| {
                if let Some(tint_view) = slot.as_ref() {
                    tint_view.setHidden(true);
                }
            });

            // Hide default GPUI visual effect view so it doesn't block the glass
            for subview in content_view.subviews().iter() {
                if let Some(effect_view) = subview.downcast_ref::<NSVisualEffectView>() {
                    effect_view.setHidden(true);
                }
            }

            let alpha = if active_theme.is_dark { 0.45 } else { 0.35 };
            let tint_color = NSColor::colorWithSRGBRed_green_blue_alpha(
                f64::from(rgb.r),
                f64::from(rgb.g),
                f64::from(rgb.b),
                alpha,
            );

            let glass_class = AnyClass::get(c"NSGlassView");
            if let Some(glass_class) = glass_class {
                LIQUID_GLASS_VIEW.with_borrow_mut(|slot| {
                    let needs_new_view = slot.as_ref().is_none_or(|glass_view| {
                        glass_view
                            .window()
                            .as_deref()
                            .is_none_or(|win| !std::ptr::eq(win, native_window))
                    });
                    if needs_new_view {
                        let view_alloc: *mut AnyObject = msg_send![glass_class, alloc];
                        let frame = content_view.bounds();
                        let initialized: *mut NSView = msg_send![view_alloc, initWithFrame: frame];
                        if let Some(glass_view) = unsafe { Retained::from_raw(initialized) } {
                            glass_view.setAutoresizingMask(
                                NSAutoresizingMaskOptions::ViewWidthSizable
                                    | NSAutoresizingMaskOptions::ViewHeightSizable,
                            );
                            content_view.addSubview_positioned_relativeTo(
                                &glass_view,
                                NSWindowOrderingMode::Below,
                                Some(view),
                            );
                            *slot = Some(glass_view);
                        }
                    }
                    if let Some(glass_view) = slot.as_ref() {
                        glass_view.setHidden(false);
                        let _: () = msg_send![&**glass_view, setTintColor: &*tint_color];
                    }
                });
            } else {
                // Fallback for older macOS versions
                LIQUID_GLASS_VIEW.with_borrow_mut(|slot| {
                    let needs_new_view = slot.as_ref().is_none_or(|eff| {
                        eff.window()
                            .as_deref()
                            .is_none_or(|win| !std::ptr::eq(win, native_window))
                    });
                    if needs_new_view {
                        let effect = NSVisualEffectView::initWithFrame(
                            NSVisualEffectView::alloc(main_thread),
                            content_view.bounds(),
                        );
                        effect.setMaterial(NSVisualEffectMaterial::UnderWindowBackground);
                        effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
                        effect.setState(NSVisualEffectState::Active);
                        effect.setWantsLayer(true);
                        effect.setAutoresizingMask(
                            NSAutoresizingMaskOptions::ViewWidthSizable
                                | NSAutoresizingMaskOptions::ViewHeightSizable,
                        );
                        content_view.addSubview_positioned_relativeTo(
                            &effect,
                            NSWindowOrderingMode::Below,
                            Some(view),
                        );
                        *slot = Some(Retained::into_super(effect));
                    }
                    if let Some(effect_view) = slot.as_ref() {
                        effect_view.setHidden(false);
                        if let Some(layer) = effect_view.layer() {
                            layer.setBackgroundColor(Some(&tint_color.CGColor()));
                        }
                    }
                });
            }
        }
        insulator_protocol::theme::WindowStyle::Transparent => {
            native_window.setOpaque(false);
            native_window.setHasShadow(true);
            native_window.setBackgroundColor(Some(&NSColor::clearColor()));

            LIQUID_GLASS_VIEW.with_borrow(|slot| {
                if let Some(glass_view) = slot.as_ref() {
                    glass_view.setHidden(true);
                }
            });
            SIDEBAR_TINT_VIEW.with_borrow(|slot| {
                if let Some(tint_view) = slot.as_ref() {
                    tint_view.setHidden(true);
                }
            });
            for subview in content_view.subviews().iter() {
                if let Some(effect_view) = subview.downcast_ref::<NSVisualEffectView>() {
                    effect_view.setHidden(false);
                    effect_view.setMaterial(NSVisualEffectMaterial::Sidebar);
                    effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
                    effect_view.setState(NSVisualEffectState::Active);
                }
            }
        }
        insulator_protocol::theme::WindowStyle::Image => {
            native_window.setOpaque(true);
            native_window.setHasShadow(true);
            let tint_color = NSColor::colorWithSRGBRed_green_blue_alpha(
                f64::from(rgb.r),
                f64::from(rgb.g),
                f64::from(rgb.b),
                1.0,
            );
            native_window.setBackgroundColor(Some(&tint_color));

            LIQUID_GLASS_VIEW.with_borrow(|slot| {
                if let Some(glass_view) = slot.as_ref() {
                    glass_view.setHidden(true);
                }
            });
            SIDEBAR_TINT_VIEW.with_borrow(|slot| {
                if let Some(tint_view) = slot.as_ref() {
                    tint_view.setHidden(true);
                }
            });
            for subview in content_view.subviews().iter() {
                if let Some(effect_view) = subview.downcast_ref::<NSVisualEffectView>() {
                    effect_view.setHidden(true);
                }
            }
        }
        insulator_protocol::theme::WindowStyle::Solid => {
            native_window.setOpaque(true);
            native_window.setHasShadow(true);
            let tint_color = NSColor::colorWithSRGBRed_green_blue_alpha(
                f64::from(rgb.r),
                f64::from(rgb.g),
                f64::from(rgb.b),
                1.0,
            );
            native_window.setBackgroundColor(Some(&tint_color));

            LIQUID_GLASS_VIEW.with_borrow(|slot| {
                if let Some(glass_view) = slot.as_ref() {
                    glass_view.setHidden(true);
                }
            });
            SIDEBAR_TINT_VIEW.with_borrow(|slot| {
                if let Some(tint_view) = slot.as_ref() {
                    tint_view.setHidden(false);
                }
            });
            for subview in content_view.subviews().iter() {
                if let Some(effect_view) = subview.downcast_ref::<NSVisualEffectView>() {
                    effect_view.setHidden(false);
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub fn configure_window_style(
    window: &Window,
    style: insulator_protocol::theme::WindowStyle,
    color_theme: insulator_protocol::theme::ColorTheme,
    _image_path: Option<&str>,
) {
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    let Some(main_thread) = objc2::MainThreadMarker::new() else {
        return;
    };

    CURRENT_WINDOW_STYLE.set(style);

    unsafe {
        let view = handle.ns_view.cast::<NSView>().as_ref();
        let Some(native_window) = view.window() else {
            return;
        };
        MAIN_WINDOW.with_borrow_mut(|slot| *slot = Some(native_window.clone()));
        if let Some(view_retained) =
            objc2::rc::Retained::retain(handle.ns_view.as_ptr().cast::<NSView>())
        {
            MAIN_VIEW.with_borrow_mut(|slot| *slot = Some(view_retained));
        }
        apply_native_window_style(&native_window, view, style, color_theme, main_thread);
    }
}

#[cfg(target_os = "macos")]
pub fn reapply_window_style(
    style: insulator_protocol::theme::WindowStyle,
    color_theme: insulator_protocol::theme::ColorTheme,
    _image_path: Option<&str>,
) {
    let Some(main_thread) = objc2::MainThreadMarker::new() else {
        return;
    };
    CURRENT_WINDOW_STYLE.set(style);
    MAIN_WINDOW.with_borrow(|win_slot| {
        MAIN_VIEW.with_borrow(|view_slot| {
            if let (Some(win), Some(view)) = (win_slot.as_ref(), view_slot.as_ref()) {
                unsafe {
                    apply_native_window_style(win, view, style, color_theme, main_thread);
                }
            }
        });
    });
}

#[cfg(target_os = "macos")]
thread_local! {
    static TAB_CYCLE_CALLBACK: std::cell::RefCell<Option<Box<dyn Fn(bool)>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_os = "macos")]
extern "C-unwind" fn custom_select_next_tab(
    _this: &objc2::runtime::AnyObject,
    _sel: objc2::runtime::Sel,
    _sender: *mut std::ffi::c_void,
) {
    TAB_CYCLE_CALLBACK.with_borrow(|callback| {
        if let Some(callback) = callback.as_ref() {
            callback(false);
        }
    });
}

#[cfg(target_os = "macos")]
extern "C-unwind" fn custom_select_previous_tab(
    _this: &objc2::runtime::AnyObject,
    _sel: objc2::runtime::Sel,
    _sender: *mut std::ffi::c_void,
) {
    TAB_CYCLE_CALLBACK.with_borrow(|callback| {
        if let Some(callback) = callback.as_ref() {
            callback(true);
        }
    });
}

#[cfg(target_os = "macos")]
pub fn register_tab_cycle_handler<F: Fn(bool) + 'static>(handler: F) {
    TAB_CYCLE_CALLBACK.with_borrow_mut(|slot| {
        *slot = Some(Box::new(handler));
    });

    static HOOK_ONCE: std::sync::Once = std::sync::Once::new();
    HOOK_ONCE.call_once(|| {
        use objc2::runtime::{AnyClass, AnyObject, Sel};

        for class_name in [c"GPUIWindow", c"GPUIPanel"] {
            let Some(cls) = AnyClass::get(class_name) else {
                continue;
            };

            unsafe {
                let sel_next = objc2::sel!(selectNextTab:);
                let types = c"v@:@".as_ptr();
                let _ = objc2::ffi::class_replaceMethod(
                    cls as *const AnyClass as *mut AnyClass,
                    sel_next,
                    std::mem::transmute(
                        custom_select_next_tab
                            as extern "C-unwind" fn(&AnyObject, Sel, *mut std::ffi::c_void),
                    ),
                    types,
                );

                let sel_prev = objc2::sel!(selectPreviousTab:);
                let _ = objc2::ffi::class_replaceMethod(
                    cls as *const AnyClass as *mut AnyClass,
                    sel_prev,
                    std::mem::transmute(
                        custom_select_previous_tab
                            as extern "C-unwind" fn(&AnyObject, Sel, *mut std::ffi::c_void),
                    ),
                    types,
                );
            }
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn register_tab_cycle_handler<F: Fn(bool) + 'static>(_handler: F) {}

#[cfg(not(target_os = "macos"))]
pub fn configure_window_style(
    _: &Window,
    _: insulator_protocol::theme::WindowStyle,
    _: insulator_protocol::theme::ColorTheme,
    _: Option<&str>,
) {
}

#[cfg(not(target_os = "macos"))]
pub fn reapply_window_style(
    _: insulator_protocol::theme::WindowStyle,
    _: insulator_protocol::theme::ColorTheme,
    _: Option<&str>,
) {
}

#[cfg(target_os = "macos")]
pub fn set_sidebar_material_width(window: &Window, width: f32) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    let Some(_main_thread) = MainThreadMarker::new() else {
        return;
    };

    unsafe {
        let view = handle.ns_view.cast::<NSView>().as_ref();
        let Some(native_window) = view.window() else {
            return;
        };
        SIDEBAR_TINT_VIEW.with_borrow(|slot| {
            let Some(tint_view) = slot.as_ref().filter(|tint_view| {
                tint_view
                    .window()
                    .as_deref()
                    .is_some_and(|window| std::ptr::eq(window, native_window.as_ref()))
            }) else {
                return;
            };
            let mut frame = tint_view.frame();
            frame.size.width = width.into();
            tint_view.setFrame(frame);
        });
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_sidebar_material_width(_: &Window, _: f32) {}

/// Follow macOS when `dark` is `None`, otherwise force the native titlebar,
/// traffic lights, menus, and vibrancy to the selected appearance.
#[cfg(target_os = "macos")]
pub fn set_window_appearance(window: &Window, dark: Option<bool>) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSView,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    let Some(_main_thread) = MainThreadMarker::new() else {
        return;
    };

    unsafe {
        let view = handle.ns_view.cast::<NSView>().as_ref();
        let Some(native_window) = view.window() else {
            return;
        };
        let appearance = dark.and_then(|dark| {
            NSAppearance::appearanceNamed(if dark {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            })
        });
        native_window.setAppearance(appearance.as_deref());
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_window_appearance(_: &Window, _: Option<bool>) {}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::parse_boolean_setting;

    #[test]
    fn boolean_desktop_settings_are_parsed_case_insensitively() {
        assert_eq!(parse_boolean_setting(" true\n"), Some(true));
        assert_eq!(parse_boolean_setting("OFF"), Some(false));
        assert_eq!(parse_boolean_setting("default"), None);
    }

    #[test]
    fn embedded_linux_icon_decodes_at_desktop_size() {
        let icon = super::linux_app_icon().expect("embedded PNG should decode");

        assert_eq!(icon.dimensions(), (256, 256));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use std::{borrow::Cow, path::Path};

    use super::{macos_jpeg_photo, macos_picture_path, termy_open_url};

    #[test]
    fn login_picture_path_accepts_dscl_multiline_output() {
        assert_eq!(
            macos_picture_path(b"Picture:\n /Library/User Pictures/Animals/Eagle.heic\n"),
            Some("/Library/User Pictures/Animals/Eagle.heic".into())
        );
    }

    #[test]
    fn login_jpeg_photo_decodes_dscl_hex_words() {
        assert_eq!(
            macos_jpeg_photo(b"JPEGPhoto:\n ffd8 ffe0 0010\n"),
            Some(vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10])
        );
    }

    #[test]
    fn termy_projects_use_the_new_tab_deeplink() {
        let url = url::Url::parse(&termy_open_url(Path::new("/tmp/project +%")))
            .expect("Termy deeplink should be valid");

        assert_eq!(url.scheme(), "termy");
        assert_eq!(url.host_str(), Some("new"));
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![(Cow::Borrowed("dir"), Cow::Borrowed("/tmp/project +%"))]
        );
    }
}
