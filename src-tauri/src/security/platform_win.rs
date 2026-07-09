//! Windows platform-specific security helpers.
//!
//! Sets per-file / per-directory ACLs so that only the current user can
//! read/write credential files — equivalent to 0600/0700 on Unix.
//!
//! # Design note
//!
//! We intentionally avoid the OS credential manager (Credential Manager /
//! Keychain / libsecret) to guarantee **zero system password prompts**
//! during normal LLM/MCP usage.  The local AES-256-GCM encrypted store +
//! user-only ACLs is the pragmatic, honest, and prompt-free compromise.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use crate::error::{AppError, AppResult};

// ── Win32 FFI declarations ────────────────────────────────────────────────
// We use raw `extern "system"` bindings instead of the `windows` crate for
// ACL manipulation because the windows-0.62 type system introduces friction
// (WIN32_ERROR orphan rules, PSID transmutes, HLOCAL constructors) that
// makes concise ACL code unnecessarily verbose.

#[allow(non_snake_case)]
extern "system" {
    fn GetCurrentProcess() -> windows::Win32::Foundation::HANDLE;
}

#[allow(non_snake_case)]
extern "system" {
    fn OpenProcessToken(
        ProcessHandle: windows::Win32::Foundation::HANDLE,
        DesiredAccess: u32,
        TokenHandle: *mut windows::Win32::Foundation::HANDLE,
    ) -> i32;
}

#[allow(non_snake_case)]
extern "system" {
    fn GetTokenInformation(
        TokenHandle: windows::Win32::Foundation::HANDLE,
        TokenInformationClass: u32,
        TokenInformation: *mut std::ffi::c_void,
        TokenInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> i32;
}

#[allow(non_snake_case)]
extern "system" {
    fn CloseHandle(hObject: windows::Win32::Foundation::HANDLE) -> i32;
}

#[allow(non_snake_case)]
extern "system" {
    fn GetLengthSid(pSid: *mut std::ffi::c_void) -> u32;
}

#[allow(non_snake_case)]
extern "system" {
    fn LocalFree(hMem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

#[allow(non_snake_case)]
extern "system" {
    fn SetEntriesInAclW(
        cCountOfExplicitEntries: u32,
        pListOfExplicitEntries: *const EXPLICIT_ACCESS_W,
        OldAcl: *mut std::ffi::c_void,
        NewAcl: *mut *mut std::ffi::c_void,
    ) -> u32;
}

#[allow(non_snake_case)]
extern "system" {
    fn SetNamedSecurityInfoW(
        pObjectName: *const u16,
        ObjectType: u32,
        SecurityInfo: u32,
        psidOwner: *mut std::ffi::c_void,
        psidGroup: *mut std::ffi::c_void,
        pDacl: *mut std::ffi::c_void,
        pSacl: *mut std::ffi::c_void,
    ) -> u32;
}

// ── Windows constants ─────────────────────────────────────────────────────

const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_USER: u32 = 1; // TOKEN_INFORMATION_CLASS

const SE_FILE_OBJECT: u32 = 1;

const DACL_SECURITY_INFORMATION: u32 = 0x00000004;
const PROTECTED_DACL_SECURITY_INFORMATION: u32 = 0x80000000;

const GRANT_ACCESS: u32 = 1; // GRANT_ACCESS
const TRUSTEE_IS_SID: u32 = 0;

const NO_INHERITANCE: u32 = 0;
const OBJECT_INHERIT_ACE: u32 = 0x1;
const CONTAINER_INHERIT_ACE: u32 = 0x2;

const GENERIC_ALL: u32 = 0x10000000;

// ── Win32 struct definitions ──────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
struct TRUSTEE_W {
    pMultipleTrustee: *mut std::ffi::c_void,
    MultipleTrusteeOperation: u32,
    TrusteeForm: u32,
    TrusteeType: u32,
    ptstrName: *mut u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
struct EXPLICIT_ACCESS_W {
    grfAccessPermissions: u32,
    grfAccessMode: u32,
    grfInheritance: u32,
    Trustee: TRUSTEE_W,
}

// ── Public API ────────────────────────────────────────────────────────────

/// Restrict `path` so that only the current Windows user can read & write.
///
/// `is_directory` controls whether child-creation access is granted and
/// whether the ACE is marked as inheritable by children.
pub fn set_user_only_permissions(path: &Path, is_directory: bool) -> AppResult<()> {
    // 1. ── Get current user SID ──────────────────────────────────────────
    let sid_bytes = current_user_sid()?;

    // 2. ── Build EXPLICIT_ACCESS_W entry ─────────────────────────────────
    let inheritance = if is_directory {
        OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE
    } else {
        NO_INHERITANCE
    };

    let trustee = TRUSTEE_W {
        pMultipleTrustee: std::ptr::null_mut(),
        MultipleTrusteeOperation: 0,
        TrusteeForm: TRUSTEE_IS_SID,
        TrusteeType: 0,
        ptstrName: sid_bytes.as_ptr() as *mut u16,
    };

    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: GENERIC_ALL,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: inheritance,
        Trustee: trustee,
    };

    // 3. ── Create new ACL (only the single ACE) ──────────────────────────
    let mut new_acl: *mut std::ffi::c_void = std::ptr::null_mut();
    let result = unsafe {
        SetEntriesInAclW(
            1, // one entry
            &ea,
            std::ptr::null_mut(), // no existing ACL — replace entirely
            &mut new_acl,
        )
    };

    if result != 0 {
        // result is the Win32 error code; 0 = ERROR_SUCCESS
        return Err(AppError::msg(format!(
            "无法创建 Windows ACL (错误码: {result})"
        )));
    }

    // 4. ── Apply DACL to the file/directory ──────────────────────────────
    let path_wide: Vec<u16> = OsStr::new(path.as_os_str())
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        SetNamedSecurityInfoW(
            path_wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(), // owner — keep existing
            std::ptr::null_mut(), // group — keep existing
            new_acl,
            std::ptr::null_mut(), // SACL — keep existing
        )
    };

    // 5. ── Free the ACL allocated by SetEntriesInAclW ────────────────────
    unsafe {
        LocalFree(new_acl);
    }

    if result != 0 {
        return Err(AppError::msg(format!(
            "无法设置文件权限 (错误码: {result})"
        )));
    }

    Ok(())
}

// ── Internal helpers ──────────────────────────────────────────────────────

/// Retrieve the current process user's SID as a raw byte buffer.
/// The caller must keep the buffer alive while using the SID pointer.
fn current_user_sid() -> AppResult<Vec<u8>> {
    let process = unsafe { GetCurrentProcess() };
    let mut token_handle = windows::Win32::Foundation::HANDLE::default();

    let ok = unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token_handle) };
    if ok == 0 {
        return Err(AppError::msg("OpenProcessToken 失败"));
    }

    // Get required buffer size
    let mut size_needed = 0u32;
    let _ = unsafe {
        GetTokenInformation(
            token_handle,
            TOKEN_USER,
            std::ptr::null_mut(),
            0,
            &mut size_needed,
        )
    };

    if size_needed == 0 {
        unsafe {
            let _ = CloseHandle(token_handle);
        }
        return Err(AppError::msg("GetTokenInformation 返回零缓冲区大小"));
    }

    let mut buf: Vec<u8> = vec![0u8; size_needed as usize];
    let ok = unsafe {
        GetTokenInformation(
            token_handle,
            TOKEN_USER,
            buf.as_mut_ptr() as *mut std::ffi::c_void,
            size_needed,
            &mut size_needed,
        )
    };

    unsafe {
        let _ = CloseHandle(token_handle);
    }

    if ok == 0 {
        return Err(AppError::msg("GetTokenInformation(TokenUser) 失败"));
    }

    // TOKEN_USER layout: TOKEN_USER { User: SID_AND_ATTRIBUTES { Sid: PSID, Attributes: u32 } }
    // PSID is a pointer — it's stored at offset 0 of the buf (after the struct alignment).
    // We extract just the SID bytes so the caller can use them as a raw pointer.
    let sid_ptr: *const std::ffi::c_void = unsafe {
        // buf starts with TOKEN_USER. Get the SID pointer from User.Sid
        let sid_addr = buf.as_ptr() as *const *const std::ffi::c_void;
        *sid_addr
    };

    let sid_len = unsafe { GetLengthSid(sid_ptr as *mut std::ffi::c_void) as usize };
    let mut sid_bytes = vec![0u8; sid_len];
    unsafe {
        std::ptr::copy_nonoverlapping(sid_ptr as *const u8, sid_bytes.as_mut_ptr(), sid_len);
    }

    Ok(sid_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn set_user_perms_on_directory_succeeds() {
        let dir = tempfile::tempdir().expect("temp dir");
        set_user_only_permissions(dir.path(), true).expect("set dir perms");
    }

    #[test]
    fn set_user_perms_on_file_succeeds() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, b"secret").expect("write");
        set_user_only_permissions(&file_path, false).expect("set file perms");
    }

    #[test]
    fn set_user_perms_on_nonexistent_path_fails() {
        let result = set_user_only_permissions(Path::new("Z:\\nonexistent\\path\\file.txt"), false);
        assert!(result.is_err());
    }

    /// After setting perms on a directory, child files created inside it
    /// can also be restricted (the basic round-trip works).
    #[test]
    fn child_file_inherits_restricted_perms() {
        let dir = tempfile::tempdir().expect("temp dir");
        set_user_only_permissions(dir.path(), true).expect("set dir perms");

        let child = dir.path().join("child.txt");
        fs::write(&child, b"inherited").expect("write child");
        set_user_only_permissions(&child, false).expect("set child perms");
    }
}
