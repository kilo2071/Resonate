//! Global hotkeys via the XDG GlobalShortcuts portal (works on Wayland).
//!
//! Runs on its own thread with the blocking `dbus` crate (the same libdbus the
//! tray already links — no async runtime). Activations are sent to the GTK
//! main loop over an `mpsc` channel, mirroring the tray's design.
//!
//! Shortcuts: Ctrl+Alt+Numpad digits type a tile number (e.g. 5 then 6 plays
//! tile 56 — the GTK side accumulates digits and commits after a short pause);
//! Ctrl+Alt+Numpad Enter stops everything. GNOME shows a one-time approval
//! dialog; if it rejects our preferred triggers the user can rebind them in
//! GNOME Settings.

use dbus::arg::{PropMap, RefArg, Variant};
use dbus::blocking::Connection;
use dbus::message::MatchRule;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const GS_IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
const REQ_IFACE: &str = "org.freedesktop.portal.Request";

/// What a global shortcut activation means.
pub enum HotkeyEvent {
    /// A numpad digit was typed (0–9); digits accumulate into a tile number.
    Digit(u8),
    /// Numpad Enter: stop all playback.
    StopAll,
}

/// Spawn the portal client thread. The receiver yields hotkey events.
pub fn spawn() -> mpsc::Receiver<HotkeyEvent> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("resonate-hotkeys".into())
        .spawn(move || {
            if let Err(e) = run(tx) {
                log::warn!("Global shortcuts unavailable: {e}");
            }
        })
        .ok();
    rx
}

/// Wait (processing the connection) until the portal Request at `req_path`
/// emits its Response, or `timeout` passes.
fn wait_response(
    conn: &Connection,
    req_path: dbus::Path<'static>,
    timeout: Duration,
) -> anyhow::Result<PropMap> {
    let slot: Arc<Mutex<Option<(u32, PropMap)>>> = Arc::new(Mutex::new(None));
    let slot_cb = slot.clone();
    let rule = MatchRule::new_signal(REQ_IFACE, "Response").with_path(req_path);
    let token = conn.add_match(rule, move |(code, results): (u32, PropMap), _, _| {
        if let Ok(mut g) = slot_cb.lock() {
            *g = Some((code, results));
        }
        true
    })?;

    let deadline = Instant::now() + timeout;
    let result = loop {
        conn.process(Duration::from_millis(200))?;
        if let Some((code, results)) = slot.lock().ok().and_then(|mut g| g.take()) {
            break Some((code, results));
        }
        if Instant::now() > deadline {
            break None;
        }
    };
    let _ = conn.remove_match(token);

    match result {
        Some((0, results)) => Ok(results),
        Some((code, _)) => anyhow::bail!("portal request cancelled or denied (code {code})"),
        None => anyhow::bail!("portal request timed out"),
    }
}

/// The portal identifies host apps by their systemd scope name
/// (`app[-<launcher>]-<AppID>-<RANDOM>.scope`). When launched from a terminal
/// we sit in the shell's scope and the portal refuses with "An app id is
/// required" — so move ourselves into a correctly-named transient scope first.
/// Launches via the desktop file already have one; then this is a no-op.
fn ensure_app_scope() {
    let cgroup = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    if cgroup.contains(crate::APP_ID) {
        return;
    }
    let result = (|| -> anyhow::Result<()> {
        let pid = std::process::id();
        let conn = Connection::new_session()?;
        let proxy = conn.with_proxy(
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            Duration::from_millis(3000),
        );
        let name = format!("app-gnome-{}-{}.scope", crate::APP_ID, pid);
        let props: Vec<(&str, Variant<Box<dyn RefArg>>)> =
            vec![("PIDs", Variant(Box::new(vec![pid])))];
        let aux: Vec<(&str, Vec<(&str, Variant<Box<dyn RefArg>>)>)> = Vec::new();
        let _: (dbus::Path,) = proxy.method_call(
            "org.freedesktop.systemd1.Manager",
            "StartTransientUnit",
            (name.as_str(), "fail", props, aux),
        )?;
        // The unit job is asynchronous — wait until the cgroup move actually
        // lands, or the portal will still see the old (shell) scope.
        for _ in 0..40 {
            let cg = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
            if cg.contains(crate::APP_ID) {
                log::info!("Moved into app scope {name}");
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        anyhow::bail!("cgroup migration into {name} did not complete");
    })();
    if let Err(e) = result {
        log::warn!("Could not create app scope (global shortcuts may fail): {e}");
    }
}

fn run(tx: mpsc::Sender<HotkeyEvent>) -> anyhow::Result<()> {
    ensure_app_scope();
    let conn = Connection::new_session()?;
    let sender_token = conn
        .unique_name()
        .trim_start_matches(':')
        .replace('.', "_");

    let proxy = conn.with_proxy(PORTAL_DEST, PORTAL_PATH, Duration::from_millis(5000));

    // ── CreateSession ────────────────────────────────────────────────────────
    // Subscribe to the (predictable) request path before calling, so the
    // Response can't race past us.
    let req_token = "resonate_gs_create";
    let req_path = dbus::Path::from(format!(
        "/org/freedesktop/portal/desktop/request/{sender_token}/{req_token}"
    ));
    let mut opts = PropMap::new();
    opts.insert(
        "handle_token".into(),
        Variant(Box::new(req_token.to_string())),
    );
    opts.insert(
        "session_handle_token".into(),
        Variant(Box::new("resonate".to_string())),
    );
    let _: (dbus::Path,) = proxy.method_call(GS_IFACE, "CreateSession", (opts,))?;
    let results = wait_response(&conn, req_path, Duration::from_secs(15))?;
    let session_handle = results
        .get("session_handle")
        .and_then(|v| v.0.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("no session_handle in portal response"))?;
    log::info!("GlobalShortcuts session: {session_handle}");

    // ── BindShortcuts ────────────────────────────────────────────────────────
    let mut shortcuts: Vec<(String, PropMap)> = (0..=9u8)
        .map(|d| {
            let mut p = PropMap::new();
            p.insert(
                "description".into(),
                Variant(Box::new(format!("Type digit {d} of a tile number"))),
            );
            p.insert(
                "preferred_trigger".into(),
                Variant(Box::new(format!("CTRL+ALT+KP_{d}"))),
            );
            (format!("digit-{d}"), p)
        })
        .collect();
    {
        let mut p = PropMap::new();
        p.insert(
            "description".into(),
            Variant(Box::new("Stop all playing sounds".to_string())),
        );
        p.insert(
            "preferred_trigger".into(),
            Variant(Box::new("CTRL+ALT+KP_Enter".to_string())),
        );
        shortcuts.push(("stop-all".into(), p));
    }

    let bind_token = "resonate_gs_bind";
    let bind_req_path = dbus::Path::from(format!(
        "/org/freedesktop/portal/desktop/request/{sender_token}/{bind_token}"
    ));
    let mut bind_opts = PropMap::new();
    bind_opts.insert(
        "handle_token".into(),
        Variant(Box::new(bind_token.to_string())),
    );
    let session_path = dbus::Path::from(session_handle.clone());
    let _: (dbus::Path,) = proxy.method_call(
        GS_IFACE,
        "BindShortcuts",
        (session_path, shortcuts, "", bind_opts),
    )?;
    // The desktop may show an approval dialog — wait generously.
    let _ = wait_response(&conn, bind_req_path, Duration::from_secs(300))?;
    log::info!("Global shortcuts bound (Ctrl+Alt+Numpad)");

    // ── Activations ──────────────────────────────────────────────────────────
    let rule = MatchRule::new_signal(GS_IFACE, "Activated");
    conn.add_match(
        rule,
        move |(session, id, _ts, _opts): (dbus::Path, String, u64, PropMap), _, _| {
            let _ = session;
            if id == "stop-all" {
                let _ = tx.send(HotkeyEvent::StopAll);
            } else if let Some(d) = id.strip_prefix("digit-").and_then(|s| s.parse::<u8>().ok()) {
                if d <= 9 {
                    let _ = tx.send(HotkeyEvent::Digit(d));
                }
            }
            true
        },
    )?;

    loop {
        conn.process(Duration::from_millis(1000))?;
    }
}
