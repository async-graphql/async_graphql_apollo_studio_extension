
#[cfg(not(target_arch = "wasm32"))]
pub fn uname() -> std::io::Result<String> {
    let x = uname::uname()?;
    Ok(format!(
        "{sysname} {version} {release} {machine} {nodename}",
        sysname = x.sysname,
        version = x.version,
        release = x.release,
        machine = x.machine,
        nodename = x.nodename
    ))
}

#[cfg(target_arch = "wasm32")]
pub fn uname() -> std::io::Result<String> {
    Err(std::io::Error::new(std::io::ErrorKind::NotFound, "not supported on wasm32"))
}