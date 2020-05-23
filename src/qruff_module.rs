use std::ops::Deref;
use std::os::raw::c_int;
use std::slice;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Sender, Receiver, channel};

use crate::{
    ffi, mem, ClassId, ContextRef, ForeignTypeRef, MsgType, RJSTimerHandler, RuffCtx, Runtime,
    RuntimeRef, Value, RJSPromise, register_rtu_context_class, qruff_rtu_setup,
};

lazy_static! {
    static ref QRUFF_TIMER_CLASS_ID: ClassId = Runtime::new_class_id();
    static ref QRUFF_CMD_PIPE_CLASS_ID: ClassId = Runtime::new_class_id();
    static ref QRUFF_CMD_GENERATOR_CLASS_ID: ClassId = Runtime::new_class_id();
}

macro_rules! register_func {
    ($type_name:ident, $c_func:ident, $argc:expr) => {
        ffi::JSCFunctionListEntry {
            name: cstr!($type_name).as_ptr(),
            prop_flags: (ffi::JS_PROP_WRITABLE | ffi::JS_PROP_CONFIGURABLE) as u8,
            def_type: ffi::JS_DEF_CFUNC as u8,
            magic: 0,
            u: ffi::JSCFunctionListEntry__bindgen_ty_1 {
                func: ffi::JSCFunctionListEntry__bindgen_ty_1__bindgen_ty_1 {
                    length: $argc as u8,
                    cproto: ffi::JSCFunctionEnum::JS_CFUNC_generic as u8,
                    cfunc: ffi::JSCFunctionType {
                        generic: Some($c_func),
                    },
                },
            },
        }
    };
}

macro_rules! register_i32_const {
    ($const_name:ident, $value:expr) => {
        ffi::JSCFunctionListEntry {
            name: cstr!($const_name).as_ptr(),
            prop_flags: ffi::JS_PROP_CONFIGURABLE as u8,
            def_type: ffi::JS_DEF_PROP_INT32 as u8,
            magic: 0,
            u: ffi::JSCFunctionListEntry__bindgen_ty_1 { i32: $value},
        }
    };
}

macro_rules! new_func_table_type {
    ($type_name:ident, $type_name_inner:ident, $func_num:expr) => {
        type $type_name_inner = [ffi::JSCFunctionListEntry; $func_num];
        struct $type_name($type_name_inner);

        impl Deref for $type_name {
            type Target = $type_name_inner;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        unsafe impl Send for $type_name {}
        unsafe impl Sync for $type_name {}
    };
}

fn qruff_timer_class_id() -> ClassId {
    *QRUFF_TIMER_CLASS_ID
}

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct Cmd {
    pub id: String,
    pub reg_offset: u16,
    pub reg_len: u16,
    pub interval: u16,
}

#[derive(Serialize, Deserialize,Clone, Debug)]
pub struct CmdList(pub Vec<Cmd>);

impl Drop for CmdList {
    fn drop(&mut self) {
        println!("> Dropping Cmdlist");
    }
}
#[derive(Debug)]
pub struct CmdGenerator {
    pub tx: Sender<Cmd>,
    rx: Option<Receiver<Cmd>>,
    pub cmds: Option<CmdList>,
}

impl CmdGenerator {
    fn new(cmds: Option<CmdList>, tx: Sender<Cmd>, rx:Option<Receiver<Cmd>>) -> CmdGenerator {
        CmdGenerator {
            tx,
            rx,
            cmds,
        }
    }
}

#[derive(Debug)]
pub struct CmdPipe {
   pub rx: Option<Receiver<Cmd>>
}

impl CmdPipe {
    fn new() -> CmdPipe {
        CmdPipe {
            rx: None,
        }
    }
}

unsafe extern "C" fn qruff_create_cmd_pipe(
    ctx: *mut ffi::JSContext,
    _this_val: ffi::JSValue,
    _argc: ::std::os::raw::c_int,
    _argv: *mut ffi::JSValue,
) -> ffi::JSValue {
    let ctxt = ContextRef::from_ptr(ctx);
    let ret = ctxt.new_object_class(*QRUFF_CMD_PIPE_CLASS_ID);
    let pipe = CmdPipe::new();
    println!("pipe is {:p}", &pipe);
    let pipe_ptr = Box::into_raw(Box::new(pipe));
    println!("pipe_ptr is {:p}", pipe_ptr);
    ret.set_opaque(pipe_ptr);

    *ret
}

unsafe extern "C" fn qruff_cmd_generator_pipe(
    ctx: *mut ffi::JSContext,
    this_val: ffi::JSValue,
    argc: ::std::os::raw::c_int,
    argv: *mut ffi::JSValue,
) -> ffi::JSValue {
    let ctxt = ContextRef::from_ptr(ctx);
    let this = Value::from(this_val);
    let args = slice::from_raw_parts(argv, argc as usize);
    let dest = Value::from(args[0]);

    let ptr = this.get_opaque::<CmdGenerator>(*QRUFF_CMD_GENERATOR_CLASS_ID);

    match &(*ptr).rx {
        Some(rx) => {
            let mut dest_ptr = dest.get_opaque::<CmdPipe>(*QRUFF_CMD_PIPE_CLASS_ID);
            let rx = (*ptr).rx.take().unwrap();
            (*dest_ptr).rx.replace(rx);
            println!("!!!! set pipe tx !!!!!");
        },
        None => {
            println!("Already run");
        }
    }

    ffi::UNDEFINED
}

unsafe extern "C" fn qruff_cmd_generator_run(
    ctx: *mut ffi::JSContext,
    _this_val: ffi::JSValue,
    _argc: ::std::os::raw::c_int,
    _argv: *mut ffi::JSValue,
) -> ffi::JSValue {
    let ctxt = ContextRef::from_ptr(ctx);
    let this = Value::from(_this_val);

    let mut ruff_ctx = ctxt.userdata::<RuffCtx>().unwrap();
    let id = ruff_ctx.as_mut().id_generator.next_id();

    let ptr = this.get_opaque::<CmdGenerator>(*QRUFF_CMD_GENERATOR_CLASS_ID);

    match &(*ptr).cmds {
        Some(_cmds) => {
            let cmd_generator = Box::new(CmdGenerator::new((*ptr).cmds.take(), (*ptr).tx.clone(), None));
            let mut request_msg = ruff_ctx.as_mut().request_msg.lock().unwrap();
            request_msg.push(MsgType::AddCmdGenerator(id, cmd_generator));
        },
        None => {
            println!("Already run");
        }
    }

    ffi::UNDEFINED
}

unsafe extern "C" fn qruff_cmd_show(
    ctx: *mut ffi::JSContext,
    _this_val: ffi::JSValue,
    _argc: ::std::os::raw::c_int,
    _argv: *mut ffi::JSValue,
) -> ffi::JSValue {
    let ctxt = ContextRef::from_ptr(ctx);
    let this = Value::from(_this_val);

    let mut ruff_ctx = ctxt.userdata::<RuffCtx>().unwrap();
    let id = ruff_ctx.as_mut().id_generator.next_id();

    let ptr = this.get_opaque::<CmdPipe>(*QRUFF_CMD_PIPE_CLASS_ID);

    match &(*ptr).rx{
        Some(_rx) => {
            let mut request_msg = ruff_ctx.as_mut().request_msg.lock().unwrap();
            let rx = (*ptr).rx.take().unwrap();
            request_msg.push(MsgType::AddCmdShower(id, rx));
        },
        None => {
            println!("Already run");
        }
    }

    ffi::UNDEFINED
}


unsafe extern "C" fn qruff_create_cmd_generator(
    ctx: *mut ffi::JSContext,
    _this_val: ffi::JSValue,
    argc: ::std::os::raw::c_int,
    argv: *mut ffi::JSValue,
) -> ffi::JSValue {
    let ctxt = ContextRef::from_ptr(ctx);
    let args = slice::from_raw_parts(argv, argc as usize);
    let arg0 = Value::from(args[0]);

    let cmd_json = match ctxt.to_cstring(&arg0) {
        Some(value) => String::from(value.to_string_lossy()),
        None => return ffi::EXCEPTION,
    };
    let cmds: CmdList = serde_json::from_str(&cmd_json).unwrap();
    let (tx, rx) = channel(100);
    let cmd_generator = Box::new(CmdGenerator::new(Some(cmds), tx, Some(rx)));
    let ret = ctxt.new_object_class(*QRUFF_CMD_GENERATOR_CLASS_ID);
    ret.set_opaque(Box::into_raw(cmd_generator));

    *ret
}

unsafe extern "C" fn qruff_clearTimeout(
    ctx: *mut ffi::JSContext,
    _this_val: ffi::JSValue,
    argc: ::std::os::raw::c_int,
    argv: *mut ffi::JSValue,
) -> ffi::JSValue {
    let ctxt = ContextRef::from_ptr(ctx);
    let args = slice::from_raw_parts(argv, argc as usize);
    let timer = Value::from(args[0]);

    let mut ruff_ctx = ctxt.userdata::<RuffCtx>().unwrap();
    let ptr = timer.get_opaque::<u32>(*QRUFF_TIMER_CLASS_ID);

    let id: u32 = *ptr;
    println!("clear timer id is {:?}", id);
    let mut request_msg = ruff_ctx.as_mut().request_msg.lock().unwrap();
    request_msg.push(MsgType::DeleteTimer(id));
    ffi::UNDEFINED
}

unsafe extern "C" fn qruff_setTimeout(
    ctx: *mut ffi::JSContext,
    this_val: ffi::JSValue,
    argc: ::std::os::raw::c_int,
    argv: *mut ffi::JSValue,
) -> ffi::JSValue {
    let ctxt = ContextRef::from_ptr(ctx);
    let this = Value::from(this_val);
    let _this = this.check_undefined();
    let args = slice::from_raw_parts(argv, argc as usize);
    let arg0 = Value::from(args[0]);
    let arg1 = Value::from(args[1]);

    let mut ruff_ctx = ctxt.userdata::<RuffCtx>().unwrap();
    let id = ruff_ctx.as_mut().id_generator.next_id();
    let ptr = Box::into_raw(Box::new(id));
    let timer = ctxt.new_object_class(*QRUFF_TIMER_CLASS_ID);
    timer.set_opaque(ptr);
    if ctxt.is_function(&arg0) {
        let delay_ms = ctxt.to_int64(&arg1).unwrap() as u64;
        let handle = RJSTimerHandler::new(id, ctxt, delay_ms, &arg0);
        let mut request_msg = ruff_ctx.as_mut().request_msg.lock().unwrap();
        request_msg.push(MsgType::AddTimer(id, handle));
    } else {
        println!("Not Function");
        return ffi::UNDEFINED;
    }
    *timer
}

unsafe extern "C" fn qruff_getAddrInfo(
    ctx: *mut ffi::JSContext,
    this_val: ffi::JSValue,
    argc: ::std::os::raw::c_int,
    argv: *mut ffi::JSValue,
) -> ffi::JSValue {
    let ctxt = ContextRef::from_ptr(ctx);
    let this = Value::from(this_val);
    let _this = this.check_undefined();
    let args = slice::from_raw_parts(argv, argc as usize);
    let arg0 = Value::from(args[0]);

    let addr = match ctxt.to_cstring(&arg0) {
        Some(value) => String::from(value.to_string_lossy()),
        None => return ffi::EXCEPTION,
    };

    let mut ruff_ctx = ctxt.userdata::<RuffCtx>().unwrap();
    let rfunc: [ffi::JSValue; 2] = [ffi::UNDEFINED; 2];
    let id = ruff_ctx.as_mut().id_generator.next_id();
    let promise = ffi::JS_NewPromiseCapability(ctxt.as_ptr(), rfunc.as_ptr() as *mut _);
    let handle = RJSPromise::new(
        id,
        ctxt,
        &Value::from(promise),
        &Value::from(rfunc[0]),
        &Value::from(rfunc[1]),
    );

    let mut request_msg = ruff_ctx.as_mut().request_msg.lock().unwrap();
    request_msg.push(MsgType::GetAddrInfo(id, addr, handle));
    promise
}

pub fn register_creat_cmd_pipe_class(rt: &RuntimeRef) -> bool {
    unsafe extern "C" fn qruff_cmd_pipe_finalizer(_rt: *mut ffi::JSRuntime, obj: ffi::JSValue) {
        let ptr = ffi::JS_GetOpaque(obj, *QRUFF_CMD_PIPE_CLASS_ID);

        trace!("free userdata {:p} @ {:?}", ptr, obj.u.ptr);
        println!("free userdata for cmd generator {:p} @ {:?}", ptr, obj.u.ptr);

        mem::drop(Box::from_raw(ptr));
    }

    rt.new_class(
        *QRUFF_CMD_PIPE_CLASS_ID,
        &ffi::JSClassDef {
            class_name: cstr!(QRuffCmdPipe).as_ptr(),
            finalizer: Some(qruff_cmd_pipe_finalizer),
            gc_mark: None,
            call: None,
            exotic: core::ptr::null_mut(),
        },
    )
}

pub fn register_cmd_generator_class(rt: &RuntimeRef) -> bool {
    unsafe extern "C" fn qruff_generator_finalizer(_rt: *mut ffi::JSRuntime, obj: ffi::JSValue) {
        let ptr = ffi::JS_GetOpaque(obj, *QRUFF_CMD_GENERATOR_CLASS_ID) as *mut CmdGenerator;

        trace!("free userdata {:p} @ {:?}", ptr, obj.u.ptr);
        println!("free userdata for cmd generator {:p} @ {:?}", ptr, obj.u.ptr);

        mem::drop(Box::from_raw(ptr));
    }

    rt.new_class(
        *QRUFF_CMD_GENERATOR_CLASS_ID,
        &ffi::JSClassDef {
            class_name: cstr!(QRuffCmdGenerator).as_ptr(),
            finalizer: Some(qruff_generator_finalizer),
            gc_mark: None,
            call: None,
            exotic: core::ptr::null_mut(),
        },
    )
}

pub fn register_timer_class(rt: &RuntimeRef) -> bool {
    unsafe extern "C" fn qruff_timer_finalizer(_rt: *mut ffi::JSRuntime, obj: ffi::JSValue) {
        let ptr = ffi::JS_GetOpaque(obj, qruff_timer_class_id());

        trace!("free userdata {:p} @ {:?}", ptr, obj.u.ptr);
        println!("free userdata {:p} @ {:?}", ptr, obj.u.ptr);

        mem::drop(Box::from_raw(ptr));
    }

    rt.new_class(
        qruff_timer_class_id(),
        &ffi::JSClassDef {
            class_name: cstr!(QRuffTimer).as_ptr(),
            finalizer: Some(qruff_timer_finalizer),
            gc_mark: None,
            call: None,
            exotic: core::ptr::null_mut(),
        },
    )
}

new_func_table_type!(QRuffModuleFuncList, ModuleFuncList, 7);
new_func_table_type!(QRuffCmdGeneratorFuncList, CmdGeneratorFuncList, 2);
new_func_table_type!(QRuffCmdPipeFuncList, CmdPipeFuncList, 1);

lazy_static! {
    static ref QRUFF_MODULE_FUNC_TABLE: QRuffModuleFuncList = QRuffModuleFuncList([
        register_i32_const!(CONST_16, 16),
        register_func!(setTimeout, qruff_setTimeout, 2),
        register_func!(clearTimeout, qruff_clearTimeout, 1),
        register_func!(getAddrInfo, qruff_getAddrInfo, 1),
        register_func!(createCmdGenerator, qruff_create_cmd_generator, 1),
        register_func!(createCmdPipe, qruff_create_cmd_pipe, 1),
        register_func!(rtu_setup, qruff_rtu_setup, 1),
    ]);

    static ref QRUFF_CMD_PIPE_FUNC_TABLE: QRuffCmdPipeFuncList = QRuffCmdPipeFuncList([
        //register_func!(show, qruff_cmd_generator_run, 0),
        register_func!(show, qruff_cmd_show, 0),
    ]);

    static ref QRUFF_CMD_GENERATOR_FUNC_TABLE: QRuffCmdGeneratorFuncList = QRuffCmdGeneratorFuncList([
        register_func!(run, qruff_cmd_generator_run, 0),
        register_func!(pipe, qruff_cmd_generator_pipe, 1),
    ]);

}

unsafe extern "C" fn js_module_dummy_init(
    _ctx: *mut ffi::JSContext,
    _m: *mut ffi::JSModuleDef,
) -> c_int {
    let ctxt = ContextRef::from_ptr(_ctx);

    if register_timer_class(ctxt.runtime()) {
        println!("Fail to register Timer Class");
    }

    if register_cmd_generator_class(ctxt.runtime()) {
        println!("Fail to register Cmd CmdGenerator Class");
    }

    if register_rtu_context_class(ctxt.runtime()) {
        println!("Fail to register rtu context Class");
    }
    let cmd_obj = ctxt.new_object();
    ffi::JS_SetPropertyFunctionList(_ctx, cmd_obj.raw(),
        QRUFF_CMD_GENERATOR_FUNC_TABLE.as_ptr() as *mut _,
        QRUFF_CMD_GENERATOR_FUNC_TABLE.0.len() as i32,
    );

    ctxt.set_class_proto(*QRUFF_CMD_GENERATOR_CLASS_ID, cmd_obj);

    let cmd_pipe_obj = ctxt.new_object();
    ffi::JS_SetPropertyFunctionList(_ctx, cmd_pipe_obj.raw(),
        QRUFF_CMD_PIPE_FUNC_TABLE.as_ptr() as *mut _,
        QRUFF_CMD_PIPE_FUNC_TABLE.0.len() as i32,
    );

    ctxt.set_class_proto(*QRUFF_CMD_PIPE_CLASS_ID, cmd_pipe_obj);

    ffi::JS_SetModuleExportList(
        _ctx,
        _m,
        QRUFF_MODULE_FUNC_TABLE.as_ptr() as *mut _,
        QRUFF_MODULE_FUNC_TABLE.0.len() as i32,
    )
}

pub fn js_init_module_qruff(ctxt: &ContextRef, module_name: &str) {
    let m = ctxt
        .new_c_module(module_name, Some(js_module_dummy_init))
        .ok();

    unsafe {
        ffi::JS_AddModuleExportList(
            ctxt.as_ptr(),
            m.unwrap().as_ptr(),
            QRUFF_MODULE_FUNC_TABLE.as_ptr() as *mut _,
            QRUFF_MODULE_FUNC_TABLE.0.len() as i32,
        );
    }
}
