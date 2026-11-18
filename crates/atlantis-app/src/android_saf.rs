//! Android's Storage Access Framework (docs/ANDROID.md): the folder the
//! player chooses in the system's picker, read through the activity's
//! Java helpers (packaging/android/java/.../AtlantisActivity.java) over
//! JNI. Each document is opened once as a file descriptor, then read with
//! `pread` like a file on disk: a game's archives and CD images are read
//! by parts, never whole.

use std::collections::HashMap;
use std::ffi::{c_void, CStr, CString};
use std::fs::File;
use std::os::fd::FromRawFd;
use std::os::unix::fs::FileExt;
use std::sync::{Arc, Mutex};

use atlantis_data::disc_image::{DiscImage, DiscSet, ImageBytes, IMAGE_EXTENSIONS};
use atlantis_data::FileSource;
use jni_sys::{
    jclass, jobject, jstring, jvalue, JNIEnv, JNINativeMethod, JavaVM, JNI_OK, JNI_VERSION_1_6,
};

/// The Java VM and the activity (a global reference android-activity
/// keeps), as addresses.
static ACTIVITY: Mutex<Option<(usize, usize)>> = Mutex::new(None);

/// The picker's answer, until the event loop takes it: the folder's tree
/// URI, or "" when the player chose none.
static PICKED: Mutex<Option<String>> = Mutex::new(None);

/// Starts the JNI side with the current activity (each new activity of
/// the process): its `folderPicked` given its Rust function.
pub(crate) fn init(vm: *mut c_void, activity: *mut c_void) {
    *ACTIVITY.lock().unwrap() = Some((vm as usize, activity as usize));
    let registered = with_env(|env, act| unsafe {
        let cls = call(env, |f| f.GetObjectClass)?(env, act);
        let methods = [
            JNINativeMethod {
                name: c"folderPicked".as_ptr() as *mut _,
                signature: c"(Ljava/lang/String;)V".as_ptr() as *mut _,
                fnPtr: folder_picked as *mut c_void,
            },
            JNINativeMethod {
                name: c"textTyped".as_ptr() as *mut _,
                signature: c"(Ljava/lang/String;)V".as_ptr() as *mut _,
                fnPtr: text_typed as *mut c_void,
            },
            JNINativeMethod {
                name: c"keyTyped".as_ptr() as *mut _,
                signature: c"(I)V".as_ptr() as *mut _,
                fnPtr: key_typed as *mut c_void,
            },
            JNINativeMethod {
                name: c"imeHeight".as_ptr() as *mut _,
                signature: c"(I)V".as_ptr() as *mut _,
                fnPtr: ime_height_changed as *mut c_void,
            },
            JNINativeMethod {
                name: c"backPressed".as_ptr() as *mut _,
                signature: c"()V".as_ptr() as *mut _,
                fnPtr: back_pressed as *mut c_void,
            },
        ];
        let r = call(env, |f| f.RegisterNatives)?(env, cls, methods.as_ptr(), methods.len() as i32);
        call(env, |f| f.DeleteLocalRef)?(env, cls);
        (r == JNI_OK).then_some(())
    });
    if registered.is_none() {
        log::warn!("the activity's Java helpers are not there: no folder picker");
    }
}

/// `AtlantisActivity.folderPicked(String)`.
extern "system" fn folder_picked(env: *mut JNIEnv, _class: jclass, uri: jstring) {
    let uri = unsafe { rust_string(env, uri) }.unwrap_or_default();
    log::info!(
        "folder picked: {}",
        if uri.is_empty() { "none" } else { &uri }
    );
    *PICKED.lock().unwrap() = Some(uri);
}

/// What the activity passed on from the on-screen keyboard and the Back
/// button, in order, until the event loop takes it.
#[derive(Debug, PartialEq)]
pub(crate) enum Typed {
    Text(String),
    /// Android's key code (66 Enter, 67 Backspace).
    Key(i32),
    Back,
    /// (only a redraw: the keyboard's height changed)
    Redraw,
}

static TYPED: Mutex<Vec<Typed>> = Mutex::new(Vec::new());

/// The event loop woken for what [`TYPED`] got (a user event: winit's
/// own wake-up is ignored without a redraw due).
static WAKE: Mutex<Option<winit::event_loop::EventLoopProxy<()>>> = Mutex::new(None);

/// The event loop to wake when the activity passes something on.
pub(crate) fn set_waker(proxy: winit::event_loop::EventLoopProxy<()>) {
    *WAKE.lock().unwrap() = Some(proxy);
}

fn typed(t: Typed) {
    TYPED.lock().unwrap().push(t);
    if let Some(p) = WAKE.lock().unwrap().as_ref() {
        let _ = p.send_event(());
    }
}

/// What came since the last call.
pub(crate) fn take_typed() -> Vec<Typed> {
    std::mem::take(&mut *TYPED.lock().unwrap())
}

/// `AtlantisActivity.textTyped(String)`.
extern "system" fn text_typed(env: *mut JNIEnv, _class: jclass, text: jstring) {
    if let Some(t) = unsafe { rust_string(env, text) } {
        log::debug!("typed {t:?}");
        typed(Typed::Text(t));
    }
}

/// `AtlantisActivity.keyTyped(int)`.
extern "system" fn key_typed(_env: *mut JNIEnv, _class: jclass, code: jni_sys::jint) {
    log::debug!("typed key {code}");
    typed(Typed::Key(code));
}

/// The on-screen keyboard's height over the window, in pixels (0: hidden).
static IME_HEIGHT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub(crate) fn ime_height() -> usize {
    IME_HEIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

/// `AtlantisActivity.imeHeight(int)`: the picture is drawn above it.
extern "system" fn ime_height_changed(_env: *mut JNIEnv, _class: jclass, h: jni_sys::jint) {
    let h = usize::try_from(h).unwrap_or(0);
    if IME_HEIGHT.swap(h, std::sync::atomic::Ordering::Relaxed) != h {
        log::info!("on-screen keyboard: {h} pixels high");
        typed(Typed::Redraw);
    }
}

/// `AtlantisActivity.backPressed()`.
extern "system" fn back_pressed(_env: *mut JNIEnv, _class: jclass) {
    log::info!("Back");
    typed(Typed::Back);
}

/// Shows or hides the on-screen keyboard (typing into the activity's
/// text view).
pub(crate) fn show_keyboard(show: bool) -> bool {
    with_env(|env, act| unsafe {
        let m = method(env, act, c"showKeyboard", c"(Z)V")?;
        call(env, |f| f.CallVoidMethodA)?(env, act, m, [jvalue { z: u8::from(show) }].as_ptr());
        no_exception(env)
    })
    .is_some()
}

/// The picker's answer, once.
pub(crate) fn take_picked() -> Option<String> {
    PICKED.lock().unwrap().take()
}

/// Opens the system's folder picker.
pub(crate) fn pick_folder() -> bool {
    with_env(|env, act| unsafe { call_void(env, act, c"pickFolder", c"()V", &[]) }).is_some()
}

/// Whether the app may still read the folder `tree` (a permission kept).
pub(crate) fn can_read(tree: &str) -> bool {
    with_env(|env, act| unsafe {
        let t = java_string(env, tree)?;
        let f = call(env, |f| f.CallBooleanMethodA)?;
        let m = method(env, act, c"canRead", c"(Ljava/lang/String;)Z")?;
        let r = f(env, act, m, [jvalue { l: t }].as_ptr());
        no_exception(env)?;
        Some(r != 0)
    })
    .unwrap_or(false)
}

/// A document of a folder.
#[derive(Clone, Debug)]
struct Entry {
    name: String,
    doc: String,
    dir: bool,
    size: u64,
}

fn root_document(tree: &str) -> Option<String> {
    with_env(|env, act| unsafe {
        let t = java_string(env, tree)?;
        call_string(
            env,
            act,
            c"rootDocument",
            c"(Ljava/lang/String;)Ljava/lang/String;",
            &[t],
        )
    })
}

fn list_children(tree: &str, doc: &str) -> Option<Vec<Entry>> {
    with_env(|env, act| unsafe {
        let t = java_string(env, tree)?;
        let d = java_string(env, doc)?;
        let m = method(
            env,
            act,
            c"listChildren",
            c"(Ljava/lang/String;Ljava/lang/String;)[Ljava/lang/String;",
        )?;
        let arr = call(env, |f| f.CallObjectMethodA)?(
            env,
            act,
            m,
            [jvalue { l: t }, jvalue { l: d }].as_ptr(),
        );
        no_exception(env)?;
        if arr.is_null() {
            return None;
        }
        let n = call(env, |f| f.GetArrayLength)?(env, arr);
        let mut out = Vec::new();
        for i in 0..n {
            let s = call(env, |f| f.GetObjectArrayElement)?(env, arr, i);
            let line = rust_string(env, s);
            call(env, |f| f.DeleteLocalRef)?(env, s);
            let line = line?;
            let mut parts = line.split('\t');
            let (Some(name), Some(doc), Some(dir), Some(size)) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            out.push(Entry {
                name: name.to_string(),
                doc: doc.to_string(),
                dir: dir == "1",
                size: size.parse().unwrap_or(0),
            });
        }
        Some(out)
    })
}

fn open_document(tree: &str, doc: &str) -> Option<File> {
    let fd = with_env(|env, act| unsafe {
        let t = java_string(env, tree)?;
        let d = java_string(env, doc)?;
        let m = method(
            env,
            act,
            c"openDocument",
            c"(Ljava/lang/String;Ljava/lang/String;)I",
        )?;
        let fd = call(env, |f| f.CallIntMethodA)?(
            env,
            act,
            m,
            [jvalue { l: t }, jvalue { l: d }].as_ptr(),
        );
        no_exception(env)?;
        Some(fd)
    })?;
    // SAFETY: a descriptor detached from its ParcelFileDescriptor: ours.
    (fd >= 0).then(|| unsafe { File::from_raw_fd(fd) })
}

/// A folder of the chosen tree, as the game's files (names matched
/// without regard to case, as [`atlantis_data::StdDir`] does).
pub(crate) struct SafDir {
    tree: String,
    doc: String,
    /// Folder document -> its documents.
    children: Mutex<HashMap<String, Vec<Entry>>>,
    /// Lower-case relative path -> document (`None`: not there).
    index: Mutex<HashMap<String, Option<Entry>>>,
    open: Mutex<HashMap<String, Arc<File>>>,
}

impl SafDir {
    /// The top of the tree `tree` (a tree URI the picker gave).
    pub(crate) fn top(tree: &str) -> Option<SafDir> {
        Some(SafDir::at(tree, &root_document(tree)?))
    }

    fn at(tree: &str, doc: &str) -> SafDir {
        SafDir {
            tree: tree.to_string(),
            doc: doc.to_string(),
            children: Mutex::new(HashMap::new()),
            index: Mutex::new(HashMap::new()),
            open: Mutex::new(HashMap::new()),
        }
    }

    fn children_of(&self, doc: &str) -> Vec<Entry> {
        if let Some(c) = self.children.lock().unwrap().get(doc) {
            return c.clone();
        }
        let c = list_children(&self.tree, doc).unwrap_or_default();
        self.children
            .lock()
            .unwrap()
            .insert(doc.to_string(), c.clone());
        c
    }

    fn key(name: &str) -> String {
        name.replace('\\', "/")
            .trim_matches('/')
            .to_ascii_lowercase()
    }

    fn resolve(&self, name: &str) -> Option<Entry> {
        let key = Self::key(name);
        if let Some(e) = self.index.lock().unwrap().get(&key) {
            return e.clone();
        }
        let mut found: Option<Entry> = None;
        let mut doc = self.doc.clone();
        for part in key.split('/').filter(|p| !p.is_empty()) {
            let e = self
                .children_of(&doc)
                .into_iter()
                .find(|e| e.name.to_ascii_lowercase() == part);
            match e {
                Some(e) => {
                    doc = e.doc.clone();
                    found = Some(e);
                }
                None => {
                    found = None;
                    break;
                }
            }
        }
        self.index.lock().unwrap().insert(key, found.clone());
        found
    }

    fn file(&self, e: &Entry) -> Option<Arc<File>> {
        if let Some(f) = self.open.lock().unwrap().get(&e.doc) {
            return Some(f.clone());
        }
        let f = Arc::new(open_document(&self.tree, &e.doc)?);
        self.open.lock().unwrap().insert(e.doc.clone(), f.clone());
        Some(f)
    }

    /// The folders directly inside this one, in name order.
    pub(crate) fn subfolders(&self) -> Vec<SafDir> {
        let mut dirs: Vec<Entry> = self
            .children_of(&self.doc)
            .into_iter()
            .filter(|e| e.dir)
            .collect();
        dirs.sort_by_key(|e| e.name.to_lowercase());
        dirs.iter()
            .map(|e| SafDir::at(&self.tree, &e.doc))
            .collect()
    }

    /// The CD images in this folder (holding an ISO 9660 file system), as
    /// one set, `None` when there are none.
    pub(crate) fn disc_set(&self) -> Option<DiscSet> {
        let mut files: Vec<Entry> = self
            .children_of(&self.doc)
            .into_iter()
            .filter(|e| {
                !e.dir
                    && e.name.rsplit_once('.').is_some_and(|(_, ext)| {
                        IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
                    })
            })
            .collect();
        files.sort_by(|a, b| a.name.cmp(&b.name));
        let images: Vec<DiscImage> = files
            .iter()
            .filter_map(|e| {
                let f = open_document(&self.tree, &e.doc)?;
                DiscImage::from_bytes(e.name.as_str(), Box::new(FdBytes(f)))
            })
            .collect();
        (!images.is_empty()).then_some(DiscSet { images })
    }
}

impl FileSource for SafDir {
    fn size(&self, name: &str) -> Option<u64> {
        let e = self.resolve(name)?;
        (!e.dir).then_some(e.size)
    }

    fn read_at(&self, name: &str, offset: u64, len: usize) -> Option<Vec<u8>> {
        let e = self.resolve(name)?;
        let f = self.file(&e)?;
        let mut buf = vec![0; len];
        f.read_exact_at(&mut buf, offset).ok()?;
        Some(buf)
    }

    fn has_dir(&self, name: &str) -> bool {
        self.resolve(name).is_some_and(|e| e.dir)
    }

    fn list(&self, dir: &str) -> Vec<String> {
        let doc = if Self::key(dir).is_empty() {
            Some(self.doc.clone())
        } else {
            self.resolve(dir).filter(|e| e.dir).map(|e| e.doc)
        };
        doc.map(|d| self.children_of(&d).into_iter().map(|e| e.name).collect())
            .unwrap_or_default()
    }
}

/// A CD image's bytes, read from its document.
struct FdBytes(File);

impl ImageBytes for FdBytes {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> bool {
        self.0.read_exact_at(buf, offset).is_ok()
    }
}

// ---------------------------------------------------------------------
// JNI

type Fns = jni_sys::JNINativeInterface_;

/// A function of the JNI table.
unsafe fn call<T>(env: *mut JNIEnv, pick: impl FnOnce(&Fns) -> Option<T>) -> Option<T> {
    pick(&**env)
}

/// Runs `f` with this thread's JNI environment (attached if it was not)
/// and the activity, in a local frame; a Java exception is cleared and
/// gives `None`.
fn with_env<T>(f: impl FnOnce(*mut JNIEnv, jobject) -> Option<T>) -> Option<T> {
    let (vm, act) = (*ACTIVITY.lock().unwrap())?;
    let vm = vm as *mut JavaVM;
    unsafe {
        let mut env: *mut c_void = std::ptr::null_mut();
        let get_env = (**vm).GetEnv?;
        if get_env(vm, &mut env, JNI_VERSION_1_6) != JNI_OK {
            let attach = (**vm).AttachCurrentThread?;
            if attach(vm, &mut env, std::ptr::null_mut()) != JNI_OK {
                return None;
            }
        }
        let env = env as *mut JNIEnv;
        if call(env, |f| f.PushLocalFrame)?(env, 32) != JNI_OK {
            return None;
        }
        let r = f(env, act as jobject);
        let _ = no_exception(env);
        call(env, |f| f.PopLocalFrame)?(env, std::ptr::null_mut());
        r
    }
}

/// `None` (the exception logged and cleared) when a Java exception is
/// pending.
unsafe fn no_exception(env: *mut JNIEnv) -> Option<()> {
    if call(env, |f| f.ExceptionCheck)?(env) != 0 {
        call(env, |f| f.ExceptionDescribe)?(env);
        call(env, |f| f.ExceptionClear)?(env);
        return None;
    }
    Some(())
}

unsafe fn method(
    env: *mut JNIEnv,
    obj: jobject,
    name: &CStr,
    sig: &CStr,
) -> Option<jni_sys::jmethodID> {
    let cls = call(env, |f| f.GetObjectClass)?(env, obj);
    let m = call(env, |f| f.GetMethodID)?(env, cls, name.as_ptr(), sig.as_ptr());
    call(env, |f| f.DeleteLocalRef)?(env, cls);
    no_exception(env)?;
    (!m.is_null()).then_some(m)
}

unsafe fn call_void(
    env: *mut JNIEnv,
    obj: jobject,
    name: &CStr,
    sig: &CStr,
    args: &[jobject],
) -> Option<()> {
    let m = method(env, obj, name, sig)?;
    let a: Vec<jvalue> = args.iter().map(|&l| jvalue { l }).collect();
    call(env, |f| f.CallVoidMethodA)?(env, obj, m, a.as_ptr());
    no_exception(env)
}

unsafe fn call_string(
    env: *mut JNIEnv,
    obj: jobject,
    name: &CStr,
    sig: &CStr,
    args: &[jobject],
) -> Option<String> {
    let m = method(env, obj, name, sig)?;
    let a: Vec<jvalue> = args.iter().map(|&l| jvalue { l }).collect();
    let s = call(env, |f| f.CallObjectMethodA)?(env, obj, m, a.as_ptr());
    no_exception(env)?;
    rust_string(env, s)
}

unsafe fn java_string(env: *mut JNIEnv, s: &str) -> Option<jstring> {
    let c = CString::new(s).ok()?;
    let j = call(env, |f| f.NewStringUTF)?(env, c.as_ptr());
    (!j.is_null()).then_some(j)
}

unsafe fn rust_string(env: *mut JNIEnv, s: jstring) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let chars = call(env, |f| f.GetStringUTFChars)?(env, s, std::ptr::null_mut());
    if chars.is_null() {
        return None;
    }
    let out = CStr::from_ptr(chars).to_string_lossy().into_owned();
    call(env, |f| f.ReleaseStringUTFChars)?(env, s, chars);
    Some(out)
}
