//! Apeireth 桌面伙伴 — 薄 Tauri shell
//!
//! 窗口管理 + 托盘 + 通知 + 全局快捷键.
//! **Agent runtime 不在这里** — 对话/记忆/工具/宪法全部由 apeireth-companion
//! 后端承担 (companion_serve :8090 OpenAI 兼容端点). 本壳只负责桌面承载.

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WebviewUrl, WebviewWindowBuilder,
};

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

#[tauri::command]
fn open_settings(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn toggle_quick_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("quick") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn build_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "打开主窗", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quick", "快捷窗口", true, None::<&str>)?;
    Menu::with_items(app, &[&show, &quick, &quit])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例: 二次启动聚焦已有主窗而不是再开一个 (尽量靠前注册).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![ping, open_settings, toggle_quick_window])
        .setup(|app| {
            let handle = app.handle().clone();

            // 后端自拉起 (v1): 探测 :8090 /health, 未在听则 spawn companion_serve.
            // 独立线程执行, 不阻塞 UI; env 整体继承父进程, 密钥不入码不打印.
            std::thread::spawn(ensure_backend_running);

            // 主窗口由 tauri.conf.json 声明 (app.windows[0] label=main), 这里不再重复创建.

            // 快捷窗 (Alt+Space 呼出, 先只建主窗足够; 后续 Phase 2 加 quick window)
            let _ = WebviewWindowBuilder::new(app, "quick", WebviewUrl::App("index.html?window=quick".into()))
                .title("Apeireth 快捷")
                .inner_size(440.0, 390.0)
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false)
                .build();

            // 托盘
            let menu = build_menu(&handle)?;
            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Apeireth 伙伴")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quick" => toggle_quick_window(app.clone()),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(&handle)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭主窗时隐藏到托盘, 不退出 (桌面伴随体常驻)
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running companion-desktop");
}

/// 后端自拉起 (v1). 在独立线程里跑, 不阻塞 UI.
///
/// 行为契约:
/// - 探测 `127.0.0.1:8090/health` 返回 200 → 已在跑, 直接返回.
/// - 端口被占但 /health 非 200 → 疑似被别的进程占用, 不 spawn (避免端口冲突),
///   前端健康门控会自然显示未连接.
/// - 连不上 → spawn `target/debug/examples/companion_serve.exe` (相对仓库根解析).
///
/// 纪律: env 整体继承父进程, APEIRETH_API_KEY 等密钥不入码不打印; spawn 失败只记日志.
fn ensure_backend_running() {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let probe = || -> Result<String, std::io::Error> {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8090));
        let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(500))?;
        s.set_read_timeout(Some(Duration::from_millis(800)))?;
        s.write_all(b"GET /health HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")?;
        let mut buf = [0u8; 128];
        let n = s.read(&mut buf).unwrap_or(0);
        Ok(String::from_utf8_lossy(&buf[..n]).into_owned())
    };

    match probe() {
        Ok(head) if head.contains(" 200") => {
            println!("[companion-shell] 后端 :8090 /health 已在听, 跳过自拉起");
            return;
        }
        Ok(_) => {
            println!("[companion-shell] :8090 被占用但 /health 非 200, 跳过自拉起 (交给前端门控)");
            return;
        }
        Err(_) => println!("[companion-shell] :8090 未在听, 尝试拉起 companion_serve…"),
    }

    // 相对仓库根解析 dev 后端路径: src-tauri → companion-desktop → frontend → 仓库根.
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| manifest_dir.join("../../.."));
    let exe = repo_root
        .join("target")
        .join("debug")
        .join("examples")
        .join("companion_serve.exe");
    if !exe.is_file() {
        eprintln!(
            "[companion-shell] 未找到后端 exe: {} (需先 cargo build --example companion_serve)",
            exe.display()
        );
        return;
    }

    let mut cmd = std::process::Command::new(&exe);
    cmd.current_dir(&repo_root); // 后端可能按 cwd 解析配置/数据目录
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: 后台静默, 不弹黑窗
    }
    match cmd.spawn() {
        Ok(child) => {
            let pid = child.id();
            println!("[companion-shell] companion_serve 已拉起 pid={pid}, 等待端口就绪…");
            std::thread::sleep(Duration::from_secs(2));
            match probe() {
                Ok(head) if head.contains(" 200") => {
                    println!("[companion-shell] 后端已就绪 (pid={pid})")
                }
                _ => eprintln!("[companion-shell] 拉起后 :8090 仍未就绪, 前端将显示未连接 (v1 可接受)"),
            }
        }
        Err(e) => eprintln!("[companion-shell] spawn 失败: {e} (前端将显示未连接, v1 可接受)"),
    }
}
