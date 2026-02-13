use std::os::windows::process::CommandExt;
use std::process::Command;

/// Launch a Windows application
pub fn launch_app(path: &str, args: &[String]) -> Result<(), anyhow::Error> {
    Command::new(path)
        .args(args)
        .creation_flags(0x00000008) // DETACHED_PROCESS
        .spawn()?;
    
    Ok(())
}

/// Open a URL in the default browser
pub fn open_url(url: &str) -> Result<(), anyhow::Error> {
    open::that(url)?;
    Ok(())
}

/// Open a file with the default application
pub fn open_file(path: &str) -> Result<(), anyhow::Error> {
    open::that(path)?;
    Ok(())
}

/// Check if a process is running
pub fn is_process_running(process_name: &str) -> bool {
    use std::process::Command;
    
    let output = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {}", process_name)])
        .output();
    
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains(process_name)
        }
        Err(_) => false,
    }
}

/// Kill a process by name
pub fn kill_process(process_name: &str) -> Result<(), anyhow::Error> {
    Command::new("taskkill")
        .args(["/IM", process_name, "/F"])
        .output()?;
    
    Ok(())
}
