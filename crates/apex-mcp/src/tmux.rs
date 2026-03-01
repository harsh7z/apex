use std::process::Command;

/// Check if we're inside a tmux session
pub fn in_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Spawn a new tmux pane running apex-tui
pub fn spawn_pane(
    widget_type: &str,
    socket_path: &str,
    position: &str,
    size_percent: u32,
    path: Option<&str>,
) -> Result<String, String> {
    if !in_tmux() {
        return Err("Not inside a tmux session. Apex requires tmux.".to_string());
    }

    let apex_tui = find_apex_tui_binary()?;

    let mut cmd_str = format!(
        "{} --widget {} --socket {}",
        apex_tui, widget_type, socket_path
    );
    if let Some(p) = path {
        cmd_str.push_str(&format!(" --path '{}'", p.replace('\'', "'\\''")));
    }

    let split_flag = match position {
        "bottom" => "-v",
        _ => "-h", // "right" is default
    };

    // Switch focus to the new widget pane so user can interact immediately
    let output = Command::new("tmux")
        .args([
            "split-window",
            split_flag,
            "-l",
            &format!("{}%", size_percent),
            "-P",
            "-F",
            "#{pane_id}",
            &cmd_str,
        ])
        .output()
        .map_err(|e| format!("Failed to run tmux: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tmux split-window failed: {}", stderr));
    }

    let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(pane_id)
}

/// Spawn a tmux pane running mcat for high-quality image display
pub fn spawn_image_pane(
    image_path: &str,
    position: &str,
    size_percent: u32,
) -> Result<String, String> {
    if !in_tmux() {
        return Err("Not inside a tmux session. Apex requires tmux.".to_string());
    }

    // Find mcat
    let mcat = find_binary("mcat")
        .ok_or_else(|| "mcat not found. Install it: cargo install mcat".to_string())?;

    // Use mcat with inline output + kitty protocol, then wait for keypress
    // The bash wrapper: display image, then wait so the pane stays open
    let cmd_str = format!(
        "clear && {} -i --kitty '{}' && echo '' && echo 'Press q to close' && read -n 1 -s key && exit",
        mcat, image_path
    );

    let split_flag = match position {
        "bottom" => "-v",
        _ => "-h",
    };

    let output = Command::new("tmux")
        .args([
            "split-window",
            "-d",
            split_flag,
            "-l",
            &format!("{}%", size_percent),
            "-P",
            "-F",
            "#{pane_id}",
            "bash",
            "-c",
            &cmd_str,
        ])
        .output()
        .map_err(|e| format!("Failed to run tmux: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tmux split-window failed: {}", stderr));
    }

    let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(pane_id)
}

/// Kill a tmux pane
pub fn kill_pane(pane_id: &str) -> Result<(), String> {
    let output = Command::new("tmux")
        .args(["kill-pane", "-t", pane_id])
        .output()
        .map_err(|e| format!("Failed to kill pane: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tmux kill-pane failed: {}", stderr));
    }

    Ok(())
}

fn find_binary(name: &str) -> Option<String> {
    Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn find_apex_tui_binary() -> Result<String, String> {
    // Check common locations
    let candidates = [
        // Same directory as apex-mcp
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("apex-tui")))
            .map(|p| p.display().to_string()),
        // In PATH
        Some("apex-tui".to_string()),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate == "apex-tui" {
            if find_binary("apex-tui").is_some() {
                return Ok(candidate);
            }
        } else if std::path::Path::new(&candidate).exists() {
            return Ok(candidate);
        }
    }

    Err("apex-tui binary not found. Make sure it's in the same directory as apex-mcp or in PATH.".to_string())
}
