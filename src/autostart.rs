use std::mem::size_of;
use std::slice;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    REG_VALUE_TYPE,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("CrosshairOverlay");

pub fn is_enabled() -> Result<bool, String> {
    let Some(command) = read_command()? else {
        return Ok(false);
    };
    let current = std::env::current_exe()
        .map_err(|err| format!("Не удалось определить путь приложения: {err}"))?;
    let registered = registered_exe_path(&command);
    Ok(registered.eq_ignore_ascii_case(current.to_string_lossy().as_ref()))
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        let current = std::env::current_exe()
            .map_err(|err| format!("Не удалось определить путь приложения: {err}"))?;
        let command = format!("\"{}\"", current.display());
        let data: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = unsafe {
            slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * size_of::<u16>())
        };

        let mut key = HKEY::default();
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(win32_error("Не удалось открыть раздел автозапуска", status));
        }
        let status = unsafe { RegSetValueExW(key, VALUE_NAME, None, REG_SZ, Some(bytes)) };
        unsafe {
            let _ = RegCloseKey(key);
        }
        if status != ERROR_SUCCESS {
            return Err(win32_error("Не удалось записать автозапуск", status));
        }
        Ok(())
    } else {
        let mut key = HKEY::default();
        let status =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key) };
        if status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND {
            return Ok(());
        }
        if status != ERROR_SUCCESS {
            return Err(win32_error("Не удалось открыть раздел автозапуска", status));
        }
        let status = unsafe { RegDeleteValueW(key, VALUE_NAME) };
        unsafe {
            let _ = RegCloseKey(key);
        }
        if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND {
            return Err(win32_error("Не удалось удалить автозапуск", status));
        }
        Ok(())
    }
}

fn read_command() -> Result<Option<String>, String> {
    unsafe {
        let mut key = HKEY::default();
        let status = RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_QUERY_VALUE, &mut key);
        if status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(win32_error("Не удалось открыть раздел автозапуска", status));
        }

        let mut data_type = REG_VALUE_TYPE(0);
        let mut size = 0u32;
        let status = RegQueryValueExW(
            key,
            VALUE_NAME,
            None,
            Some(&mut data_type),
            None,
            Some(&mut size),
        );
        if status == ERROR_FILE_NOT_FOUND {
            let _ = RegCloseKey(key);
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            let _ = RegCloseKey(key);
            return Err(win32_error("Не удалось прочитать автозапуск", status));
        }
        if data_type != REG_SZ || size < 2 {
            let _ = RegCloseKey(key);
            return Ok(None);
        }

        let mut buffer = vec![0u16; (size as usize).div_ceil(size_of::<u16>())];
        let mut actual_size = size;
        let status = RegQueryValueExW(
            key,
            VALUE_NAME,
            None,
            Some(&mut data_type),
            Some(buffer.as_mut_ptr() as *mut u8),
            Some(&mut actual_size),
        );
        let _ = RegCloseKey(key);
        if status != ERROR_SUCCESS {
            return Err(win32_error("Не удалось прочитать автозапуск", status));
        }

        let end = buffer
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(buffer.len());
        Ok(Some(String::from_utf16_lossy(&buffer[..end])))
    }
}

fn registered_exe_path(command: &str) -> &str {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next().unwrap_or(rest)
    } else {
        command
    }
}

fn win32_error(action: &str, status: WIN32_ERROR) -> String {
    format!("{action}: код ошибки Windows {}", status.0)
}
