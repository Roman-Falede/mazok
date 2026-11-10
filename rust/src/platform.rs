//! Платформенные функции: выбор и сохранение файлов, папки, системный буфер обмена, шрифты.
//! На компьютере диалоги синхронные (rfd), на Android — системный выбор файлов (SAF), результат приходит позже.

use crate::raster::Img;
use std::path::PathBuf;
use std::sync::OnceLock;

static EXTERNAL_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_external_dir(p: PathBuf) {
    let _ = EXTERNAL_DIR.set(p);
}

pub const IS_ANDROID: bool = cfg!(target_os = "android");

/// Уникальное имя файла в папке: «имя (2).png» и т. п.
#[cfg(not(target_os = "android"))]
fn unique_path(dir: &std::path::Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() { return p; }
    let path = std::path::Path::new(name);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    for i in 2..10_000 {
        let q = dir.join(format!("{stem} ({i}){ext}"));
        if !q.exists() { return q; }
    }
    p
}

/* ---------- компьютер: диалоги ---------- */

#[cfg(not(target_os = "android"))]
fn start_dir(last: &str) -> Option<PathBuf> {
    let p = PathBuf::from(last);
    if !last.is_empty() && p.is_dir() { return Some(p); }
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Pictures")).filter(|p| p.exists())
}

/// Выбрать файлы (имя, содержимое). last_dir запоминает папку.
#[cfg(not(target_os = "android"))]
pub fn pick_files(title: &str, name: &str, exts: &[&str], multiple: bool, last_dir: &mut String) -> Vec<(String, Vec<u8>)> {
    let mut dlg = rfd::FileDialog::new().set_title(title);
    if !exts.is_empty() { dlg = dlg.add_filter(name, exts); }
    dlg = dlg.add_filter("Все файлы", &["*"]);
    if let Some(d) = start_dir(last_dir) { dlg = dlg.set_directory(d); }
    let paths = if multiple { dlg.pick_files().unwrap_or_default() } else { dlg.pick_file().into_iter().collect() };
    if let Some(parent) = paths.first().and_then(|p| p.parent()) { *last_dir = parent.to_string_lossy().to_string(); }
    read_paths(&paths)
}

pub fn read_paths(paths: &[PathBuf]) -> Vec<(String, Vec<u8>)> {
    paths.iter().filter_map(|p| {
        let n = p.file_name()?.to_string_lossy().to_string();
        std::fs::read(p).ok().map(|b| (n, b))
    }).collect()
}

/// Диалог «Сохранить как». Возвращает сообщение для пользователя; Err("") — отмена.
#[cfg(not(target_os = "android"))]
pub fn save_dialog(name: &str, bytes: &[u8], last_dir: &mut String) -> Result<String, String> {
    let ext = std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
    let mut dlg = rfd::FileDialog::new().set_title("Сохранить").set_file_name(name);
    if !ext.is_empty() { dlg = dlg.add_filter(ext.to_uppercase(), &[ext.as_str()]); }
    if let Some(d) = start_dir(last_dir) { dlg = dlg.set_directory(d); }
    let Some(path) = dlg.save_file() else { return Err(String::new()) };
    if let Some(parent) = path.parent() { *last_dir = parent.to_string_lossy().to_string(); }
    std::fs::write(&path, bytes).map_err(|e| format!("Не удалось сохранить: {e}"))?;
    Ok(format!("Сохранено: {}", path.display()))
}

/// Выбрать папку (на компьютере — сразу).
#[cfg(not(target_os = "android"))]
pub fn pick_folder(start: &str) -> Option<String> {
    let mut dlg = rfd::FileDialog::new().set_title("Папка для сохранения");
    if let Some(d) = start_dir(start) { dlg = dlg.set_directory(d); }
    dlg.pick_folder().map(|p| p.to_string_lossy().to_string())
}

/// Записать файл в выбранную папку без вопросов (имя не перезаписывает существующие).
#[cfg(not(target_os = "android"))]
pub fn write_in_folder(dir: &str, name: &str, _mime: &str, bytes: &[u8]) -> Result<String, String> {
    let d = PathBuf::from(dir);
    if !d.is_dir() { return Err("Папка для сохранения недоступна — выберите её заново в настройках".into()); }
    let p = unique_path(&d, name);
    std::fs::write(&p, bytes).map_err(|e| format!("Не удалось сохранить: {e}"))?;
    Ok(format!("Сохранено: {}", p.display()))
}

#[cfg(not(target_os = "android"))]
pub fn folder_label(dir: &str) -> String {
    dir.to_string()
}

/// Открыть папку в проводнике.
#[cfg(not(target_os = "android"))]
pub fn reveal(dir: &str) {
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
}

/* ---------- Android: системный выбор файлов ---------- */

#[cfg(target_os = "android")]
pub fn folder_label(dir: &str) -> String {
    // content://…/tree/primary%3ADCIM%2FMazok → DCIM/Mazok
    let last = dir.rsplit('/').next().unwrap_or(dir);
    let dec = percent_decode(last);
    dec.split_once(':').map(|(_, p)| if p.is_empty() { "Внутренняя память".to_string() } else { p.to_string() }).unwrap_or(dec)
}

#[cfg(target_os = "android")]
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) { out.push(v); i += 3; continue; }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(target_os = "android")]
pub mod android {
    use jni::objects::{JByteArray, JObject, JString, JValue};
    use jni::JNIEnv;

    pub struct Picked {
        pub req: i32,
        pub ok: bool,
        pub uris: Vec<String>,
    }

    fn with<R>(f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<R>) -> Result<R, String> {
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
        let r = f(&mut env, &activity);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
            return Err("Ошибка Java".into());
        }
        r.map_err(|e| e.to_string())
    }

    fn jstr(env: &mut JNIEnv, o: JObject) -> Option<String> {
        if o.is_null() { return None; }
        let s = JString::from(o);
        env.get_string(&s).ok().map(|s| s.into())
    }

    pub fn open(mimes: &str, multiple: bool) -> Result<(), String> {
        with(|env, a| {
            let m = env.new_string(mimes)?;
            env.call_method(a, "openDocuments", "(Ljava/lang/String;Z)V", &[JValue::Object(&m), JValue::Bool(multiple as u8)])?;
            Ok(())
        })
    }

    pub fn create(name: &str, mime: &str) -> Result<(), String> {
        with(|env, a| {
            let n = env.new_string(name)?;
            let m = env.new_string(mime)?;
            env.call_method(a, "createDocument", "(Ljava/lang/String;Ljava/lang/String;)V", &[JValue::Object(&n), JValue::Object(&m)])?;
            Ok(())
        })
    }

    pub fn tree() -> Result<(), String> {
        with(|env, a| { env.call_method(a, "openTree", "()V", &[])?; Ok(()) })
    }

    pub fn poll() -> Option<Picked> {
        let s = with(|env, a| {
            let cls = env.get_object_class(a)?;
            let r = env.call_static_method(cls, "takeResult", "()Ljava/lang/String;", &[])?.l()?;
            Ok(jstr(env, r))
        }).ok()??;
        let mut it = s.split('\n');
        let req = it.next()?.trim().parse().ok()?;
        let ok = it.next()?.trim() == "1";
        let uris = it.filter(|u| !u.trim().is_empty()).map(|u| u.trim().to_string()).collect();
        Some(Picked { req, ok, uris })
    }

    fn bytes_of(env: &mut JNIEnv, o: JObject) -> Option<Vec<u8>> {
        if o.is_null() { return None; }
        let arr = JByteArray::from(o);
        env.convert_byte_array(&arr).ok()
    }

    pub fn read(uri: &str) -> Option<(String, Vec<u8>)> {
        with(|env, a| {
            let u = env.new_string(uri)?;
            let name = env.call_method(a, "displayName", "(Ljava/lang/String;)Ljava/lang/String;", &[JValue::Object(&u)])?.l()?;
            let name = jstr(env, name).unwrap_or_else(|| "file".into());
            let data = env.call_method(a, "readUri", "(Ljava/lang/String;)[B", &[JValue::Object(&u)])?.l()?;
            Ok(bytes_of(env, data).map(|b| (name, b)))
        }).ok().flatten()
    }

    pub fn write(uri: &str, bytes: &[u8]) -> Result<(), String> {
        let ok = with(|env, a| {
            let u = env.new_string(uri)?;
            let arr = env.byte_array_from_slice(bytes)?;
            Ok(env.call_method(a, "writeUri", "(Ljava/lang/String;[B)Z", &[JValue::Object(&u), JValue::Object(&arr)])?.z()?)
        })?;
        if ok { Ok(()) } else { Err("Не удалось записать файл".into()) }
    }

    pub fn create_in_tree(tree: &str, name: &str, mime: &str) -> Result<String, String> {
        with(|env, a| {
            let t = env.new_string(tree)?;
            let n = env.new_string(name)?;
            let m = env.new_string(mime)?;
            let r = env.call_method(a, "createInTree", "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;", &[JValue::Object(&t), JValue::Object(&n), JValue::Object(&m)])?.l()?;
            Ok(jstr(env, r))
        })?.ok_or_else(|| "Нет доступа к папке — выберите её заново в настройках".into())
    }

    pub fn clipboard_image() -> Option<Vec<u8>> {
        with(|env, a| {
            let r = env.call_method(a, "clipboardImage", "()[B", &[])?.l()?;
            Ok(bytes_of(env, r))
        }).ok().flatten()
    }
}

/// Сохранение в «Галерею» / «Загрузки» через MediaStore (запасной путь на Android).
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

#[cfg(target_os = "android")]
fn android_media_store(name: &str, mime: &str, bytes: &[u8]) -> Result<String, String> {
    use jni::objects::{JObject, JValue};
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let img = mime.starts_with("image/") && !mime.contains("photoshop");
    let (dir, class) = if img { ("Pictures/Мазок", "android/provider/MediaStore$Images$Media") } else { ("Download/Мазок", "android/provider/MediaStore$Downloads") };
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
        if uri.is_null() { return Err(jni::errors::Error::NullPtr("uri")); }
        let os = env.call_method(&resolver, "openOutputStream", "(Landroid/net/Uri;)Ljava/io/OutputStream;", &[JValue::Object(&uri)])?.l()?;
        let arr = env.byte_array_from_slice(bytes)?;
        env.call_method(&os, "write", "([B)V", &[JValue::Object(&arr)])?;
        env.call_method(&os, "close", "()V", &[])?;
        Ok(format!("Сохранено: {dir}/{name}"))
    })();
    if env.exception_check().unwrap_or(false) { let _ = env.exception_clear(); }
    res.map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
pub fn write_in_folder(dir: &str, name: &str, mime: &str, bytes: &[u8]) -> Result<String, String> {
    let uri = android::create_in_tree(dir, name, mime)?;
    android::write(&uri, bytes)?;
    Ok(format!("Сохранено: {}/{}", folder_label(dir), name))
}

/* ---------- буфер обмена ---------- */

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    pub static PASTE: AtomicBool = AtomicBool::new(false);
    pub static OUR_SEQ: AtomicU32 = AtomicU32::new(u32::MAX);
    type HookProc = unsafe extern "system" fn(i32, usize, isize) -> isize;
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowsHookExW(id: i32, f: HookProc, hmod: isize, thread: u32) -> isize;
        fn CallNextHookEx(h: isize, code: i32, w: usize, l: isize) -> isize;
        fn GetKeyState(vk: i32) -> i16;
        fn GetClipboardSequenceNumber() -> u32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        x: i32,
        y: i32,
    }
    /// Хук WH_GETMESSAGE: видит каждое сообщение, которое окно забирает из очереди.
    unsafe extern "system" fn hook(code: i32, w: usize, l: isize) -> isize {
        // w == PM_REMOVE: сообщение действительно забрано (а не просто подсмотрено)
        if code >= 0 && w == 1 && l != 0 {
            let m = &*(l as *const Msg);
            if m.message == 0x0100 && (m.lparam >> 30) & 1 == 0 {
                let ctrl = GetKeyState(0x11) < 0;
                let shift = GetKeyState(0x10) < 0;
                let alt = GetKeyState(0x12) < 0;
                if (m.wparam == 0x56 && ctrl && !alt) || (m.wparam == 0x2D && shift && !ctrl) {
                    PASTE.store(true, Ordering::Relaxed);
                }
            }
        }
        CallNextHookEx(0, code, w, l)
    }
    /// Перехват Ctrl+V для текущего потока (egui сам не сообщает о вставке картинок).
    pub fn install() {
        unsafe { SetWindowsHookExW(3, hook, 0, GetCurrentThreadId()); }
    }
    pub fn seq() -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }
}

pub fn install_paste_hook() {
    #[cfg(windows)]
    win::install();
}

/// Была ли нажата комбинация вставки (Ctrl+V, Shift+Insert) с прошлого вызова.
pub fn take_paste_request() -> bool {
    #[cfg(windows)]
    { return win::PASTE.swap(false, std::sync::atomic::Ordering::Relaxed); }
    #[allow(unreachable_code)]
    false
}

/// Картинка из системного буфера обмена.
pub fn clipboard_image() -> Option<Img> {
    #[cfg(not(target_os = "android"))]
    {
        let mut cb = arboard::Clipboard::new().ok()?;
        let im = cb.get_image().ok()?;
        if im.width == 0 || im.height == 0 { return None; }
        return Some(Img::from_straight(im.width, im.height, &im.bytes));
    }
    #[cfg(target_os = "android")]
    {
        let b = android::clipboard_image()?;
        crate::formats::decode_image(&b).ok()
    }
}

/// Файлы, скопированные в проводнике.
pub fn clipboard_files() -> Vec<PathBuf> {
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            if let Ok(v) = cb.get().file_list() { return v; }
        }
    }
    vec![]
}

/// Положить картинку в системный буфер обмена (чтобы вставить в другую программу).
pub fn set_clipboard_image(img: &Img) -> bool {
    #[cfg(not(target_os = "android"))]
    {
        let Ok(mut cb) = arboard::Clipboard::new() else { return false };
        let data = arboard::ImageData { width: img.w, height: img.h, bytes: std::borrow::Cow::Owned(img.to_straight()) };
        let ok = cb.set_image(data).is_ok();
        #[cfg(windows)]
        if ok { win::OUR_SEQ.store(win::seq(), std::sync::atomic::Ordering::Relaxed); }
        return ok;
    }
    #[allow(unreachable_code)]
    {
        let _ = img;
        false
    }
}

/// Содержимое буфера обмена положили мы сами (тогда вставляем на то же место).
pub fn clipboard_is_ours() -> bool {
    #[cfg(windows)]
    { return win::OUR_SEQ.load(std::sync::atomic::Ordering::Relaxed) == win::seq(); }
    #[allow(unreachable_code)]
    true
}

/* ---------- шрифты ---------- */

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
