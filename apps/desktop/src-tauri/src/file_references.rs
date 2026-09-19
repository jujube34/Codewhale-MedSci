//! Native file references replace the GUI's image-only, base64 attachment path.
//! References enter append-only user history; the session's frozen prefix is unchanged.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReference {
    pub path: String,
    pub is_directory: bool,
    #[serde(default)]
    pub slot: u8,
}

fn inspect(path: String) -> Result<FileReference, String> {
    if !Path::new(&path).is_absolute() || path.chars().any(char::is_control) {
        return Err("文件引用必须是有效的绝对路径，且不能含换行或控制字符".into());
    }
    let metadata = std::fs::metadata(&path).map_err(|e| format!("无法访问 {path}：{e}"))?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(format!("不支持的文件类型：{path}"));
    }
    Ok(FileReference {
        path,
        is_directory: metadata.is_dir(),
        slot: 0,
    })
}

#[tauri::command]
pub fn clipboard_file_references() -> Result<Vec<FileReference>, String> {
    resolve_file_references(native_paths()?)
}

#[tauri::command]
pub fn resolve_file_references(paths: Vec<String>) -> Result<Vec<FileReference>, String> {
    if paths.len() > 10 {
        return Err("一次最多引用 10 个文件/文件夹".into());
    }
    paths.into_iter().map(inspect).collect()
}

pub fn prepend(text: String, references: Vec<FileReference>) -> Result<String, String> {
    if references.len() > 10 || text.len() > 128 * 1024 {
        return Err("任务或引用超过限制（最多 10 个文件/文件夹）".into());
    }
    let mut used = [false; 10];
    let mut lines = Vec::new();
    for reference in references {
        let slot = usize::from(reference.slot);
        if slot >= 10 || used[slot] {
            return Err("文件引用编号无效或重复".into());
        }
        used[slot] = true;
        let current = inspect(reference.path)?;
        if current.is_directory != reference.is_directory {
            return Err(format!("引用类型已改变，请重新粘贴：{}", current.path));
        }
        let kind = if current.is_directory {
            "文件夹"
        } else {
            "文件"
        };
        lines.push(format!(
            "{kind}{}：{}",
            char::from(b'A' + reference.slot),
            current.path
        ));
    }
    if lines.is_empty() {
        return Ok(text);
    }
    Ok(format!("{}\n\n{text}", lines.join("\n")))
}

#[cfg(windows)]
fn native_paths() -> Result<Vec<String>, String> {
    use windows_sys::Win32::{
        System::DataExchange::{
            CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        },
        UI::Shell::DragQueryFileW,
    };
    const CF_HDROP: u32 = 15;
    // The clipboard owns HDROP. Keep it open while copying UTF-16 paths, never free it.
    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP) == 0 {
            return Ok(vec![]);
        }
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("剪贴板正忙，请重新粘贴".into());
        }
        struct ClipboardGuard;
        impl Drop for ClipboardGuard {
            fn drop(&mut self) {
                unsafe {
                    CloseClipboard();
                }
            }
        }
        let _guard = ClipboardGuard;
        let handle = GetClipboardData(CF_HDROP);
        if handle.is_null() {
            return Err("无法读取剪贴板中的文件".into());
        }
        let count = DragQueryFileW(handle, u32::MAX, std::ptr::null_mut(), 0);
        if count > 10 {
            return Err("一次最多引用 10 个文件/文件夹".into());
        }
        (0..count)
            .map(|index| {
                let len = DragQueryFileW(handle, index, std::ptr::null_mut(), 0);
                if len == 0 || len > 32767 {
                    return Err("剪贴板文件路径无效".into());
                }
                let mut buffer = vec![0u16; len as usize + 1];
                if DragQueryFileW(handle, index, buffer.as_mut_ptr(), len + 1) != len {
                    return Err("剪贴板文件路径读取不完整".into());
                }
                String::from_utf16(&buffer[..len as usize])
                    .map_err(|_| "文件路径不是有效 Unicode".into())
            })
            .collect()
    }
}

#[cfg(target_os = "macos")]
fn native_paths() -> Result<Vec<String>, String> {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;
    let pasteboard = NSPasteboard::generalPasteboard();
    let Some(items) = pasteboard.pasteboardItems() else {
        return Ok(vec![]);
    };
    let file_url = NSString::from_str("public.file-url");
    let mut paths = Vec::new();
    for item in items.to_vec() {
        if let Some(value) = item.stringForType(&file_url) {
            let path = url::Url::parse(&value.to_string())
                .ok()
                .and_then(|url| url.to_file_path().ok())
                .and_then(|path| path.into_os_string().into_string().ok())
                .ok_or("剪贴板文件 URL 无效")?;
            paths.push(path);
            if paths.len() > 10 {
                return Err("一次最多引用 10 个文件/文件夹".into());
            }
        }
    }
    Ok(paths)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn native_paths() -> Result<Vec<String>, String> {
    Err("文件引用粘贴目前支持 Windows 和 macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_precede_prompt_and_keep_aliases_after_removal() {
        let file = std::env::current_exe()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let directory = std::env::temp_dir().to_str().unwrap().to_owned();
        let mut resolved = resolve_file_references(vec![file.clone(), directory.clone()]).unwrap();
        assert!(!resolved[0].is_directory);
        assert!(resolved[1].is_directory);
        let mut c = resolved.pop().unwrap();
        let mut a = resolved.pop().unwrap();
        a.slot = 0;
        c.slot = 2;
        assert_eq!(
            prepend("比较文件A和文件夹C".into(), vec![a, c]).unwrap(),
            format!("文件A：{file}\n文件夹C：{directory}\n\n比较文件A和文件夹C")
        );
        assert_eq!(prepend("普通任务".into(), vec![]).unwrap(), "普通任务");
    }

    #[test]
    fn rejects_forged_aliases_types_paths_and_oversized_requests() {
        let reference = inspect(std::env::current_exe().unwrap().to_str().unwrap().into()).unwrap();
        assert!(resolve_file_references(vec![reference.path.clone(); 11]).is_err());
        assert!(
            resolve_file_references(vec![reference.path.clone(), "relative.docx".into()]).is_err()
        );
        assert!(prepend(String::new(), vec![reference.clone(); 11]).is_err());
        assert!(prepend(String::new(), vec![reference.clone(); 2]).is_err());
        let mut invalid = reference.clone();
        invalid.slot = 10;
        assert!(prepend(String::new(), vec![invalid]).is_err());
        let mut invalid = reference;
        invalid.is_directory = true;
        assert!(prepend(String::new(), vec![invalid]).is_err());
        assert!(inspect("relative.docx".into()).is_err());
        assert!(inspect(format!("{}\nforged prompt", std::env::temp_dir().display())).is_err());
        assert!(
            inspect(
                std::env::temp_dir()
                    .join("missing-reference-994c28f3.docx")
                    .to_str()
                    .unwrap()
                    .into()
            )
            .is_err()
        );
        assert!(prepend("x".repeat(128 * 1024 + 1), vec![]).is_err());
    }
}
