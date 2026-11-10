//! Платформенные функции: выбор и сохранение файлов, шрифты.

use std::path::PathBuf;
use std::sync::OnceLock;

static EXTERNAL_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_external_dir(p: PathBuf) {
    let _ = EXTERNAL_DIR.set(p);
}

pub fn can_pick_files() -> bool {
    !cfg!(target_os = "android")
}

/// Выбрать файлы (имя, содержимое).
#[cfg(not(target_os = "android"))]
pub fn pick_files(title: &str, name: &str, exts: &[&str], multiple: bool) -> Vec<(String, Vec<u8>)> {
    let dlg = rfd::FileDialog::new().set_title(title).add_filter(name, exts);
    let paths = if multiple { dlg.pick_files().unwrap_or_default() } else { dlg.pick_file().into_iter().collect() };
    paths.into_iter().filter_map(|p| {
        let n = p.file_name()?.to_string_lossy().to_string();
        std::fs::read(&p).ok().map(|b| (n, b))
    }).collect()
}

#[cfg(target_os = "android")]
pub fn pick_files(_title: &str, _name: &str, _exts: &[&str], _multiple: bool) -> Vec<(String, Vec<u8>)> {
    vec![]
}

/// Сохранить файл. Возвращает сообщение для пользователя.
#[cfg(not(target_os = "android"))]
pub fn save_bytes(name: &str, _mime: &str, bytes: &[u8]) -> Result<String, String> {
    let ext = std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
    let mut dlg = rfd::FileDialog::new().set_title("Сохранить").set_file_name(name);
    if !ext.is_empty() {
        dlg = dlg.add_filter(&ext.to_uppercase(), &[ext.as_str()]);
    }
    if let Some(dir) = dirs_pictures() {
        dlg = dlg.set_directory(dir);
    }
    let Some(path) = dlg.save_file() else { return Err(String::new()) };
    std::fs::write(&path, bytes).map_err(|e| format!("Не удалось сохранить: {e}"))?;
    Ok(format!("Сохранено: {}", path.display()))
}

#[cfg(not(target_os = "android"))]
fn dirs_pictures() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Pictures")).filter(|p| p.exists())
}

#[cfg(target_os = "android")]
pub fn save_bytes(name: &str, mime: &str, bytes: &[u8]) -> Result<String, String> {
    match android_media_store(name, mime, bytes) {
        Ok(m) => Ok(m),
        Err(_) => {
            let dir = EXTERNAL_DIR.get().cloned().unwrap_or_else(|| PathBuf::from("/sdcard/Download")).join("Мазок");
            let _ = std::fs::create_dir_all(&dir);
            let p = dir.join(name);
            std::fs::write(&p, bytes).map_err(|e| format!("Не удалось сохранить: {e}"))?;
            Ok(format!("Сохранено: {}", p.display()))
        }
    }
}

/// Сохранение в «Галерею» / «Загрузки» через MediaStore (Android 10+).
#[cfg(target_os = "android")]
fn android_media_store(name: &str, mime: &str, bytes: &[u8]) -> Result<String, String> {
    use jni::objects::{JObject, JValue};
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let img = mime.starts_with("image/") && !mime.contains("photoshop");
    let vid = mime.starts_with("video/");
    let (dir, class) = if img {
        ("Pictures/Мазок", "android/provider/MediaStore$Images$Media")
    } else if vid {
        ("Movies/Мазок", "android/provider/MediaStore$Video$Media")
    } else {
        ("Download/Мазок", "android/provider/MediaStore$Downloads")
    };
    let res: Result<String, jni::errors::Error> = (|| {
        let cv = env.new_object("android/content/ContentValues", "()V", &[])?;
        for (k, v) in [("_display_name", name), ("mime_type", mime), ("relative_path", dir)] {
            let jk = env.new_string(k)?;
            let jv = env.new_string(v)?;
            env.call_method(&cv, "put", "(Ljava/lang/String;Ljava/lang/String;)V", &[JValue::Object(&jk), JValue::Object(&jv)])?;
        }
        let coll = env.get_static_field(class, "EXTERNAL_CONTENT_URI", "Landroid/net/Uri;")?.l()?;
        let resolver = env.call_method(&activity, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
        let uri = env.call_method(&resolver, "insert", "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;", &[JValue::Object(&coll), JValue::Object(&cv)])?.l()?;
        if uri.is_null() {
            return Err(jni::errors::Error::NullPtr("uri"));
        }
        let os = env.call_method(&resolver, "openOutputStream", "(Landroid/net/Uri;)Ljava/io/OutputStream;", &[JValue::Object(&uri)])?.l()?;
        let arr = env.byte_array_from_slice(bytes)?;
        env.call_method(&os, "write", "([B)V", &[JValue::Object(&arr)])?;
        env.call_method(&os, "close", "()V", &[])?;
        Ok(format!("Сохранено: {dir}/{name}"))
    })();
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    res.map_err(|e| e.to_string())
}

/// Доступные шрифты для текста: (название, путь). Пустой путь — встроенный шрифт.
pub fn fonts() -> Vec<(String, Option<PathBuf>)> {
    let mut out = vec![("Встроенный".to_string(), None)];
    let list: &[(&str, &str)] = if cfg!(target_os = "android") {
        &[("Roboto", "/system/fonts/Roboto-Regular.ttf"), ("Noto Serif", "/system/fonts/NotoSerif-Regular.ttf"), ("Моноширинный", "/system/fonts/DroidSansMono.ttf"),
          ("Cutive Mono", "/system/fonts/CutiveMono.ttf"), ("Coming Soon", "/system/fonts/ComingSoon.ttf"), ("Dancing Script", "/system/fonts/DancingScript-Regular.ttf")]
    } else {
        &[("Arial", "C:\\Windows\\Fonts\\arial.ttf"), ("Arial Bold", "C:\\Windows\\Fonts\\arialbd.ttf"), ("Segoe UI", "C:\\Windows\\Fonts\\segoeui.ttf"),
          ("Times New Roman", "C:\\Windows\\Fonts\\times.ttf"), ("Georgia", "C:\\Windows\\Fonts\\georgia.ttf"), ("Verdana", "C:\\Windows\\Fonts\\verdana.ttf"),
          ("Trebuchet MS", "C:\\Windows\\Fonts\\trebuc.ttf"), ("Courier New", "C:\\Windows\\Fonts\\cour.ttf"), ("Comic Sans MS", "C:\\Windows\\Fonts\\comic.ttf"),
          ("Impact", "C:\\Windows\\Fonts\\impact.ttf"), ("Segoe Script", "C:\\Windows\\Fonts\\segoesc.ttf")]
    };
    for (n, p) in list {
        let p = PathBuf::from(p);
        if p.exists() {
            out.push((n.to_string(), Some(p)));
        }
    }
    out
}
