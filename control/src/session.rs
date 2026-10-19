use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

// Never Debug: credentials must not appear in logs or errors.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub version: u8,
    pub port: u16,
    pub token: String,
}
impl Connection {
    pub fn new(port: u16) -> Result<Self, String> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| "Unable to generate automation credentials")?;
        Ok(Self {
            version: crate::VERSION,
            port,
            token: bytes.iter().map(|b| format!("{b:02x}")).collect(),
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != crate::VERSION
            || self.port == 0
            || self.token.len() != 64
            || !self.token.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Invalid or incompatible automation connection file".into());
        }
        Ok(())
    }
    pub fn read(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|_| "Cannot read automation connection. Start DynamicLight and enable Local automation in Settings.")?;
        if bytes.len() > 4096 {
            return Err("Invalid automation connection file".into());
        }
        let result: Self =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid automation connection file")?;
        result.validate()?;
        Ok(result)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let parent = path.parent().ok_or("Missing automation directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        protect(parent, true)?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        // Restrict access before any token bytes are written, including on Windows.
        protect(file.path(), false)?;
        file.write_all(&serde_json::to_vec(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub fn default_path() -> Result<PathBuf, String> {
    Ok(dirs::data_dir()
        .ok_or("Cannot find user application data directory")?
        .join("com.dynamiclight.desktop/automation/session.json"))
}
#[cfg(unix)]
fn protect(path: &Path, directory: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
    )
    .map_err(|e| e.to_string())
}
#[cfg(windows)]
fn protect(path: &Path, _directory: bool) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::{w, PCWSTR},
        Win32::{
            Foundation::{LocalFree, HLOCAL},
            Security::{
                Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
                SetFileSecurityW, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
                PSECURITY_DESCRIPTOR,
            },
        },
    };
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // Protected ACL: file owner and LocalSystem only, no inherited read access.
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            w!("D:P(A;;FA;;;SY)(A;;FA;;;OW)"),
            1,
            &mut descriptor,
            None,
        )
        .map_err(|e| e.to_string())?;
        let result = SetFileSecurityW(
            PCWSTR(name.as_ptr()),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
        .ok();
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        result.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_are_rotated_saved_and_validated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation/session.json");
        let first = Connection::new(1234).unwrap();
        first.save(&path).unwrap();
        assert_eq!(Connection::read(&path).unwrap().token, first.token);
        let second = Connection::new(1235).unwrap();
        second.save(&path).unwrap();
        assert_ne!(first.token, Connection::read(&path).unwrap().token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(path.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        assert!(Connection {
            version: 1,
            port: 1,
            token: "\r\n".into()
        }
        .validate()
        .is_err());
    }
}
