use std::ops::Deref;
use std::os::raw::c_int;
use std::slice;

use crate::{
    ffi, mem, ClassId, ContextRef, ForeignTypeRef, MsgType, RJSTimerHandler, RuffCtx, Runtime,
    RuntimeRef, Value, RJSPromise
};

lazy_static! {
    static ref QRUFF_TIMER_CLASS_ID: ClassId = Runtime::new_class_id();
}

macro_rules! register_func {
    ($func_name:ident, $c_func:ident, $argc:expr) => {
        ffi::JSCFunctionListEntry {
            name: cstr!($func_name).as_ptr(),
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

fn qruff_timer_class_id() -> ClassId {
    *QRUFF_TIMER_CLASS_ID
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

type FunctionListTable = [ffi::JSCFunctionListEntry; 4];

lazy_static! {
    static ref QRUFF_MODULE_FUNC_TABLE: QRuffFunctionList = QRuffFunctionList([
        ffi::JSCFunctionListEntry {
            name: cstr!(CONST_16).as_ptr(),
            prop_flags: ffi::JS_PROP_CONFIGURABLE as u8,
            def_type: ffi::JS_DEF_PROP_INT32 as u8,
            magic: 0,
            u: ffi::JSCFunctionListEntry__bindgen_ty_1 { i32: 16 },
        },
        register_func!(setTimeout, qruff_setTimeout, 2),
        register_func!(clearTimeout, qruff_clearTimeout, 1),
        register_func!(getAddrInfo, qruff_getAddrInfo, 1),
    ]);
}

struct QRuffFunctionList(FunctionListTable);
impl Deref for QRuffFunctionList {
    type Target = FunctionListTable;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

unsafe impl Send for QRuffFunctionList {}
unsafe impl Sync for QRuffFunctionList {}

unsafe extern "C" fn js_module_dummy_init(
    _ctx: *mut ffi::JSContext,
    _m: *mut ffi::JSModuleDef,
) -> c_int {
    let ctxt = ContextRef::from_ptr(_ctx);

    if register_timer_class(ctxt.runtime()) {
        println!("Fail to register Timer Class");
    }

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
