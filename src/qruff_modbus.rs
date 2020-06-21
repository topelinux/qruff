use std::ops::{Deref, DerefMut};
use std::{slice, fmt};
use failure::Error;
use tokio::sync::mpsc::Sender;
use tokio_modbus::client::{rtu, Context, Reader};
use tokio_serial::{Serial, SerialPortSettings};

use crate::{
    ffi, mem, ClassId, ContextRef, MsgType, RuffCtx, Runtime,
    RuntimeRef, Value, RJSPromise, RespType, Args, ForeignTypeRef
};

#[derive(Debug)]
pub struct SerialConfig {
    path: String,
    settings: SerialPortSettings,
}

pub struct RtuContext(Context);

//pub struct RtuContextPtr(Context);
//impl Deref for RtuContextPtr {
//    type Target = Context;
//
//    fn deref(&self) -> &Self::Target {
//        &self.0
//    }
//}
//
//impl DerefMut for RtuContextPtr {
//    fn deref_mut(&mut self) -> &mut Self::Target {
//        &mut self.0
//    }
//}
//
//impl fmt::Debug for RtuContextPtr{
//    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//        f.debug_struct("RtuContextPtr")
//         .field("context", &"inner pointer")
//         .finish()
//    }
//}
//
//unsafe impl Send for RtuContextPtr {}
//unsafe impl Sync for RtuContextPtr {}
//
impl fmt::Debug for RtuContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RtuContext")
         .field("context", &"inner pointer")
         .finish()
    }
}

pub fn qruff_rtu_setup_settle_promise<'a>(promise: RJSPromise<'a>, context: Result<RtuContext, Error>) {
    let (handle, args) = match context {
        Ok(context) => {
            let rtu_setup = promise.ctxt.new_object_class(*QRUFF_RTU_CONTEXT_CLASS_ID);
            rtu_setup.set_opaque(Box::into_raw(Box::new(context)));
            let args = rtu_setup.into_values(&promise.ctxt);
            (&promise.resolve, args)
        },
        Err(err) => {
            let mut resp_err = String::new();
            resp_err.push_str(&format!("QJS Error {:?}", err));
            let args = resp_err.into_values(&promise.ctxt);
            (&promise.reject, args)
        }
    };
    unsafe {
        ffi::JS_Call(
            promise.ctxt.as_ptr(),
            handle.raw(),
            ffi::NULL,
            1 as i32,
            args.as_ptr() as *mut _,
        );
    }
    for arg in &args {
        promise.ctxt.free_value(*arg);
    }
}

#[derive(Debug)]
pub enum RtuOperation {
    ReadHoldingRegister(Box<RtuContext>, u16, u16),
}

unsafe impl Send for RtuOperation {}
unsafe impl Sync for RtuOperation {}

unsafe fn raw_byte_access(s16: &mut [u16]) -> &mut [u8] {
    slice::from_raw_parts_mut(s16.as_mut_ptr() as *mut u8, s16.len() * 2)
}

pub fn qruff_rtu_operation_settle_promise<'a>(promise: RJSPromise<'a>, content: Result<Vec<u16>, Error>) {
    let (handle, args) = match content {
        Ok(mut content) => {
            unsafe {
                let bytes = raw_byte_access(content.as_mut());
                (&promise.resolve, promise.ctxt.new_array_buffer_copy(bytes).into_values(&promise.ctxt))
            }
        },
        Err(err) => {
            let mut resp_err = String::new();
            resp_err.push_str(&format!("QJS Error {:?}", err));
            let args = resp_err.into_values(&promise.ctxt);
            (&promise.reject, args)
        }
    };
    unsafe {
        ffi::JS_Call(
            promise.ctxt.as_ptr(),
            handle.raw(),
            ffi::NULL,
            1 as i32,
            args.as_ptr() as *mut _,
        );
    }
    for arg in &args {
        promise.ctxt.free_value(*arg);
    }
}

pub unsafe extern "C" fn qruff_rtu_read_holding_registers(
    ctx: *mut ffi::JSContext,
    this_val: ffi::JSValue,
    argc: ::std::os::raw::c_int,
    argv: *mut ffi::JSValue,
) -> ffi::JSValue {
    let ctxt = ContextRef::from_ptr(ctx);
    let this = Value::from(this_val);
    let args = slice::from_raw_parts(argv, argc as usize);
    let arg0 = Value::from(args[0]);
    let arg1 = Value::from(args[1]);

    let addr = ctxt.to_int32(&arg0).unwrap() as u16;
    let reg_len = ctxt.to_int32(&arg1).unwrap() as u16;
    let mut ruff_ctx = ctxt.userdata::<RuffCtx>().unwrap();

    let ptr = this.get_opaque::<RtuContext>(*QRUFF_RTU_CONTEXT_CLASS_ID);
    let rfunc: [ffi::JSValue; 2] = [ffi::UNDEFINED; 2];
    let ret = unsafe {
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
        request_msg.push(MsgType::AddRtuOperation(id,
                                                  RtuOperation::ReadHoldingRegister(Box::from_raw(ptr), addr, reg_len),
                                                  handle));
        promise
    };
    ret
}

pub async fn rtu_operation(operation: RtuOperation, mut tx: Sender<RespType>, job_id: u32) {
    match operation {
        RtuOperation::ReadHoldingRegister(mut context, addr, reg_len) => {
            match context.as_mut().0.read_holding_registers(addr, reg_len).await {
                Ok(content) => {
                    tx.try_send(RespType::RtuReadHoldingRegisters(job_id, Ok(content))).unwrap();
                },
                Err(err) => {
                    tx.try_send(RespType::RtuReadHoldingRegisters(job_id, Err(err.into()))).unwrap();
                }
            }
        }
    }
}

pub unsafe extern "C" fn qruff_rtu_setup(
    ctx: *mut ffi::JSContext,
    this_val: ffi::JSValue,
    argc: ::std::os::raw::c_int,
    argv: *mut ffi::JSValue,
) -> ffi::JSValue {
    let ctxt = ContextRef::from_ptr(ctx);
    let this = Value::from(this_val);
    let args = slice::from_raw_parts(argv, argc as usize);
    let arg0 = Value::from(args[0]);
    let arg1 = Value::from(args[1]);

    let port = match ctxt.to_cstring(&arg0) {
        Some(value) => String::from(value.to_string_lossy()),
        None => return ffi::EXCEPTION,
    };

    let baud = ctxt.to_int32(&arg1).unwrap() as u32;
    let serial_config = SerialConfig {
        path: port,
        settings: SerialPortSettings {
            baud_rate: baud,
            ..Default::default()
        },
    };
    let mut ruff_ctx = ctxt.userdata::<RuffCtx>().unwrap();

    let rfunc: [ffi::JSValue; 2] = [ffi::UNDEFINED; 2];
    let ret = unsafe {
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
        request_msg.push(MsgType::CreateRtuSetup(id, serial_config, handle));
        promise
    };
    ret
}

pub async fn rtu_setup(config: SerialConfig, mut tx: Sender<RespType>, job_id: u32) {
    let port = Serial::from_path(&config.path, &config.settings);
    match port {
        Ok(port) => {
            match rtu::connect(port).await {
                Ok(context) => {
                    tx.try_send(RespType::RtuSetup(job_id, Ok(RtuContext(context)))).unwrap();
                },
                Err(err) => tx.try_send(RespType::RtuSetup(job_id, Err(err.into()))).unwrap(),
            }
        },
        Err(err) => {
            tx.try_send(RespType::RtuSetup(job_id, Err(err.into())));
        },
    }
    return;
}

pub fn qruff_rtu_context_class_id () -> ClassId {
    *QRUFF_RTU_CONTEXT_CLASS_ID
}

lazy_static! {
    static ref QRUFF_RTU_CONTEXT_CLASS_ID: ClassId = Runtime::new_class_id();
}

pub fn register_rtu_context_class(rt: &RuntimeRef) -> bool {
    unsafe extern "C" fn qruff_rtu_context_finalizer(_rt: *mut ffi::JSRuntime, obj: ffi::JSValue) {
        let ptr = ffi::JS_GetOpaque(obj, *QRUFF_RTU_CONTEXT_CLASS_ID);

        trace!("free userdata {:p} @ {:?}", ptr, obj.u.ptr);
        println!("free userdata {:p} @ {:?}", ptr, obj.u.ptr);

        mem::drop(Box::from_raw(ptr));
    }

    rt.new_class(
        *QRUFF_RTU_CONTEXT_CLASS_ID,
        &ffi::JSClassDef {
            class_name: cstr!(QRuffRtuSetup).as_ptr(),
            finalizer: Some(qruff_rtu_context_finalizer),
            gc_mark: None,
            call: None,
            exotic: core::ptr::null_mut(),
        },
    )
}
