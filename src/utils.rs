use failure::Error;
use foreign_types::ForeignTypeRef;
use foreign_types_shared::ForeignTypeRef as OtherForeignTypeRef;
use std::collections::HashMap;
use std::ffi::{CStr, OsStr};
use std::os::raw::{c_char, c_void};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;
use std::time::Duration;
use std::rc::Rc;
use std::sync::Mutex;
use tokio::fs::File;
use tokio::prelude::*;
use tokio::sync::mpsc::{channel, Sender};
use tokio::time::{delay_queue, DelayQueue};
use crate::{
    ffi, Args, Context, ContextRef, ErrorKind, Eval, Local, MallocFunctions, Runtime, Value, NewValue, Unbindable
};

pub unsafe extern "C" fn jsc_module_loader(
    ctx: *mut ffi::JSContext,
    module_name: *const c_char,
    _opaque: *mut c_void,
) -> *mut ffi::JSModuleDef {
    let ctxt = ContextRef::from_ptr(ctx);
    let module_name = Path::new(OsStr::from_bytes(CStr::from_ptr(module_name).to_bytes()));

    debug!("load module: {:?}", module_name);

    ctxt.eval_file(module_name, Eval::MODULE | Eval::COMPILE_ONLY)
        .ok()
        .map_or_else(null_mut, |func| func.as_ptr().as_ptr())
}

pub fn eval_buf<'a>(
    ctxt: &'a ContextRef,
    buf: &str,
    filename: &str,
    flags: Eval,
) -> Result<Local<'a, Value>, Error> {
    if flags.contains(Eval::MODULE) {
        let val = ctxt.eval_script(buf, filename, flags | Eval::COMPILE_ONLY)?;

        let _ = ctxt.set_import_meta(&val, true, true);

        ctxt.eval_function(val)
    } else {
        ctxt.eval_script(buf, filename, flags)
    }
}

pub async fn fs_readall_async(path: String, mut tx: Sender<RespType>, job_id: u32) {
    println!("path is {:?}", path);
    let mut file = match File::open(path).await {
        Ok(file) => file,
        Err(err) => {
            println!("err is {}", err);
            tx.try_send(RespType::FS_RESPONSE(job_id, Err(err.into())));
            return;
        }
    };
    let mut contents = vec![];
    file.read_to_end(&mut contents).await.unwrap();
    //println!("Contents in rust: {:?}", std::str::from_utf8(&contents));

    tx.send(RespType::FS_RESPONSE(job_id, Ok(contents))).await;
}

#[derive(Debug)]
pub struct RJSPromise<'a> {
    id: u32,
    ctxt: &'a ContextRef,
    p: Local<'a, Value>,
    resolve: Local<'a, Value>,
    reject: Local<'a, Value>,
}

impl<'a> RJSPromise<'a> {
    pub unsafe fn new(id: u32, ctxt: &'a ContextRef, p: &Value, resolve: &Value, reject: &Value) -> Self {
        Self {
            id,
            ctxt,
            p: ctxt.clone_value(p),
            resolve: ctxt.clone_value(resolve),
            reject: ctxt.clone_value(reject),
        }
    }
}

impl<'a> Drop for RJSPromise<'a> {
    fn drop(&mut self) {
        self.ctxt.free_value(self.resolve.raw());
        self.ctxt.free_value(self.reject.raw());
    }
}

#[derive(Debug)]
pub struct RJSTimerHandler<'a> {
    pub id: u32,
    pub ctxt: &'a ContextRef,
    pub callback: Local<'a, Value>,
    pub delay_ms: u64,
}


impl<'a> RJSTimerHandler<'a> {
    pub unsafe fn new(id: u32, ctxt: &'a ContextRef, delay_ms: u64, callback: &Value) -> Self {
        Self {
            id,
            ctxt,
            delay_ms,
            callback: ctxt.clone_value(callback),
        }
    }
}

#[derive(Debug)]
pub enum MsgType<'a> {
    AddTimer(u32,RJSTimerHandler<'a>),
    DeleteTimer(u32),
    FS_READALL(u32, String, RJSPromise<'a>),
}

#[derive(Debug)]
pub enum RespType {
    FS_RESPONSE(u32, Result<Vec<u8>, Error>),
}

type RequestMsg<'a> = Rc<Mutex<Vec<MsgType<'a>>>>;

pub struct RuffCtx<'a> {
    pub msg_tx: Sender<MsgType<'a>>,
    pub id_generator: RRIdGenerator,
    pub request_msg: RequestMsg<'a>,
}

impl<'a> RuffCtx<'a> {
    pub fn new(msg_tx: Sender<MsgType<'a>>, id_generator: RRIdGenerator, request_msg: RequestMsg<'a>) -> Self {
        RuffCtx {
            msg_tx,
            id_generator,
            request_msg,
        }
    }
}

#[derive(Debug)]
pub enum RRId {
    Timer(u32),
    Promise(u32),
}

pub struct RRIdGenerator(u32);

impl RRIdGenerator {
    pub fn new() -> Self {
        RRIdGenerator(0)
    }
    pub fn next_id(&mut self) -> u32 {
        let ret = self.0;
        self.0 += 1;
        ret
    }
}

pub struct RRIdManager<'a> {
    pending_job: HashMap<u32, RJSPromise<'a>>,
    pending_timer: HashMap<u32, delay_queue::Key>
}

unsafe impl<'a> Send for RRIdManager<'a> {}
unsafe impl<'a> Sync for RRIdManager<'a> {}

impl<'a> RRIdManager<'a> {
    pub fn new() -> Self {
        Self {
            pending_job: HashMap::new(),
            pending_timer: HashMap::new()
        }
    }

    pub fn add_timer(&mut self, timer_queue: &mut DelayQueue<RJSTimerHandler<'a>>, id: u32, timer: RJSTimerHandler<'a>) {
        let delay_ms: u64 = timer.delay_ms;
        let key = timer_queue.insert(timer, Duration::from_millis(delay_ms));
        self.pending_timer.insert(id, key);
    }

    pub fn del_timer(&mut self, timer_queue: &mut DelayQueue<RJSTimerHandler<'a>>, id: u32) {
        if let Some(key) = self.pending_timer.remove(&id) {
            println!("delete timer id {}", id);
            let item = timer_queue.remove(&key);
        } else {
            println!("Invalid id {} for delete", id);
        }
    }

    pub fn add_promise(&mut self, id: u32, promise: RJSPromise<'a>) {
        self.pending_job.insert(id, promise);
    }

    pub fn handle_response(&mut self, mut resp: Option<RespType>) {
        match resp {
            Some(RespType::FS_RESPONSE(job_id, ref mut content)) => {
                if let Some(promise) = self.pending_job.remove(&job_id) {
                    let mut resp = None;
                    let mut resp_err = String::new();
                    let handle = {
                        match content {
                            Ok(content) => {
                                resp = Some(promise.ctxt.new_array_buffer(content));
                                &promise.resolve
                            }
                            Err(err) => {
                                resp_err.push_str(&format!("QJS Error {:?}", err));
                                &promise.reject
                            }
                        }
                    };

                    unsafe {
                        if let Some(resp_to_js) = resp {
                            let args =  resp_to_js.into_values(&promise.ctxt);
                            //println!("array buffer ref count is {:?}", Value::from(args[0]).ref_cnt());
                            ffi::JS_Call(
                                promise.ctxt.as_ptr(),
                                handle.raw(),
                                ffi::NULL,
                                1 as i32,
                                args.as_ptr() as *mut _,
                            );
                            // do free for Value
                            promise.ctxt.free_value(args[0]);
                        } else {
                            ffi::JS_Call(
                                promise.ctxt.as_ptr(),
                                handle.raw(),
                                ffi::NULL,
                                1 as i32,
                                resp_err.into_values(&promise.ctxt).as_ptr() as *mut _,
                            );
                        }
                    }
                }
            }
            None => {}
        }
    }

    pub fn timer_is_empty(&self) -> bool {
        self.pending_timer.is_empty()
    }

    pub fn handle_timer(&mut self, handle: RJSTimerHandler) {
        println!("rrid is {}", handle.id);
        handle.callback.call(None, [0;0]);
        self.pending_timer.remove(&handle.id);
    }

    pub fn is_empty(&self) -> bool {
        if self.pending_timer.is_empty() {
            self.pending_job.is_empty()
        } else {
            false
        }
    }
}

pub fn check_msg_queue<'a>(request_msg: &mut RequestMsg<'a>,
                           timer_queue: &mut DelayQueue<RJSTimerHandler<'a>>,
                           resoure_manager: &mut RRIdManager<'a>,
                           resp_tx: &mut Sender<RespType>) {
    let mut request_msg = request_msg.lock().unwrap();
    let mut v = request_msg.drain(..);
    for msg in v {
        match msg{
            MsgType::AddTimer(id, handle) => resoure_manager.add_timer(timer_queue, id, handle),
            MsgType::DeleteTimer(id) => resoure_manager.del_timer(timer_queue, id),
            MsgType::FS_READALL(id, path, promise) => {
                tokio::spawn(fs_readall_async(path, resp_tx.clone(), id));
                resoure_manager.add_promise(id, promise)
            }
        }
    }
}

pub fn fs_readall(ctxt: &ContextRef, _this: Option<&Value>, args: &[Value]) -> ffi::JSValue {
    let path = String::from(ctxt.to_cstring(&args[0]).unwrap().to_string_lossy());
    let mut ruff_ctx = ctxt.userdata::<RuffCtx>().unwrap();

    println!("In Rust Function path is {}", path);
    let rfunc: [ffi::JSValue; 2] = [ffi::UNDEFINED; 2];
    let ret = unsafe {
        let id = ruff_ctx.as_mut().id_generator.next_id();
        let promise =
            ffi::JS_NewPromiseCapability(ctxt.as_ptr(), rfunc.as_ptr() as *mut _);
        let handle = RJSPromise::new(
            id,
            ctxt,
            &Value::from(promise),
            &Value::from(rfunc[0]),
            &Value::from(rfunc[1]),
        );

        let mut request_msg = ruff_ctx.as_mut().request_msg.lock().unwrap();
        request_msg.push(MsgType::FS_READALL(id, String::from(path), handle));
        promise
    };
    ret
}
