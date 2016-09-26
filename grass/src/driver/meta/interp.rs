


use std::rc::Rc;
use std::cell::RefCell;
use std::io;
use std::io::Write;


use driver::Tracer;

use bc::bytecode::{OpCode, BinOp, InternalFunc, Guard};
use core::objects::{R_BoxedValue, CallFrame, R_Pointer, R_Function, R_Struct, InstructionPointer};


#[derive(Debug, Clone, PartialEq)]
pub enum StackVal {
    Owned(R_BoxedValue),
    Ref(Rc<RefCell<R_BoxedValue>>),
}

impl StackVal {
    // to_value should be clone
    pub fn into_owned(self) -> Self {
        match self {
            StackVal::Owned(..) => self,
            StackVal::Ref(cell) => StackVal::Owned(cell.borrow().clone()),
        }
    }

    pub fn into_cell(self) -> Self {
        match self {
            StackVal::Owned(boxed) => StackVal::Ref(Rc::new(RefCell::new(boxed))),
            StackVal::Ref(..) => self,
        }
    }

    // self has to be owned
    pub fn unwrap_value(self) -> R_BoxedValue {
        if let StackVal::Owned(val) = self {
            val
        } else {
            panic!("expected owned val");
        }
    }

    pub fn unwrap_cell(self) -> Rc<RefCell<R_BoxedValue>> {
        if let StackVal::Ref(boxed) = self {
            boxed
        } else {
            panic!("expected ref val");
        }
    }

    /// Address as Value
    pub fn into_pointer(self) -> Self {
        let cell = self.unwrap_cell();
        StackVal::Owned(R_BoxedValue::Ptr(R_Pointer { cell: cell }))
    }

    /// Deref pointer
    pub fn deref(self) -> Self {
        // self contains an owned R_Pointer
        let val = self.into_owned().unwrap_value();
        if let R_BoxedValue::Ptr(ptr) = val {
            StackVal::Ref(ptr.cell)
        } else {
            panic!("expected val to be pointer, got {:?}", val);
        }
    }
}

// type Program<'a> = &'a [&'a (usize, usize, [OpCode])];
type Program = Vec<(usize, usize, Vec<OpCode>)>;



enum DispatchResult {
    Next,
    Jump(usize),
    Call(usize, usize),
    Stop,
}

pub struct Interpreter<'a> {
    pub program: &'a Program,

    // working stack of the interpreter
    pub stack: Vec<StackVal>,

    // the stack of the interpreted program, consisting of frames
    pub stack_frames: Vec<CallFrame>,
}

impl<'a> Interpreter<'a> {
    pub fn new(program: &'a Program) -> Self {
        Interpreter {
            program: program,
            stack: Vec::new(),
            stack_frames: Vec::new(),
        }
    }

    fn dispatch(&mut self, opcode: OpCode, pos: InstructionPointer) -> DispatchResult {

        match opcode {
            OpCode::Panic => panic!("assertion failed"),

            OpCode::ConstValue(val) => {
                self.stack.push(StackVal::Owned(val));
            }

            OpCode::Tuple(size) => self.o_tuple(size),
            OpCode::TupleInit(size) => self.o_tuple_init(size),
            OpCode::TupleGet(idx) => self.o_tuple_get(idx),
            OpCode::TupleSet(idx) => self.o_tuple_set(idx),

            // XXX: proper implementation of unsize
            OpCode::Unsize | OpCode::Use => {
                let val = self.stack.pop().unwrap().into_owned();
                self.stack.push(val);
            }

            OpCode::Ref => self.o_ref(),

            OpCode::Deref => self.o_deref(),

            OpCode::Load(local_index) => self.o_load(local_index),

            OpCode::Store(local_index) => self.o_store(local_index),

            OpCode::Call => {
                // load and activate func
                let func_pointer = self.o_call(pos.func, pos.pc);
                // jump to first instruction of function
                // continue is necessary because else pc += 1 would be executed
                return DispatchResult::Call(func_pointer, 0);
            }

            OpCode::Static(static_idx) => {
                let func_pointer = self.o_load_static(static_idx, pos.func, pos.pc);
                return DispatchResult::Call(func_pointer, 0);
            }

            OpCode::Return => {
                if let Some(ret) = self.o_return() {
                    return DispatchResult::Call(ret.func, ret.pc);
                } else {
                    return DispatchResult::Stop;
                }
            }

            OpCode::Skip(n) => {
                // tracer.as_mut().map(|tracer| tracer.jump_target(target));
                return DispatchResult::Jump(pos.pc + n);
            }
            OpCode::JumpBack(n) => {
                // tracer.as_mut().map(|tracer| tracer.jump_target(target));
                return DispatchResult::Jump(pos.pc - n);
            }

            OpCode::SkipIf(n) => {
                let val = self.pop_value();
                if let R_BoxedValue::Bool(b) = val {
                    if b {
                        // tracer.as_mut().map(|tracer| tracer.jump_target(pc));
                        return DispatchResult::Jump(pos.pc + n);
                    }
                } else {
                    panic!("expected bool, git {:?}", val);
                }
            }
            OpCode::JumpBackIf(n) => {
                let val = self.pop_value();
                if let R_BoxedValue::Bool(b) = val {
                    // XXX: Jumped Back
                    if b {
                        // tracer.as_mut().map(|tracer| tracer.jump_target(pc));
                        return DispatchResult::Jump(pos.pc - n);
                    }
                } else {
                    panic!("expected bool, got {:?}", val);
                }
            }

            OpCode::GetIndex => self.o_get_index(),
            OpCode::AssignIndex => self.o_assign_index(),

            OpCode::Array(size) => self.o_array(size),

            OpCode::Repeat(size) => self.o_repeat(size),

            OpCode::Len => self.o_len(),

            OpCode::BinOp(kind) => self.o_binop(kind),
            OpCode::CheckedBinOp(kind) => self.o_checked_binop(kind),

            OpCode::Not => self.o_not(),
            OpCode::Neg => unimplemented!(),
            OpCode::Noop => (),

            _ => {
                println!("XXX: {:?}", opcode);
                unimplemented!()
            }
        }

        return DispatchResult::Next;
    }

    pub fn trace(&mut self, tracer: &mut Tracer, start: InstructionPointer) {
        let mut pc: usize = start.pc;
        let mut func_pointer: usize = start.func;

        loop {
            let opcode = self.program[func_pointer].2[pc].clone();
            tracer.trace_opcode(&self, &opcode, InstructionPointer {
                func: func_pointer,
                pc: pc,
            });

            match self.dispatch(opcode, InstructionPointer{func: func_pointer, pc: pc }) {
                DispatchResult::Next => {
                    pc += 1;
                }
                DispatchResult::Jump(new_pc) => {
                    if new_pc < start.pc {
                        return;
                    }
                    pc = new_pc;
                }
                DispatchResult::Call(func, new_pc) => {
                    func_pointer = func;
                    pc = new_pc;
                }
                DispatchResult::Stop => {
                    panic!("STOPPED");
                }
            }
        }
    }


    pub fn blackhole(&mut self, start: InstructionPointer, stop: InstructionPointer) {
        let mut pc: usize = start.pc;
        let mut func_pointer = stop.func;

        loop {
            let opcode = self.program[func_pointer].2[pc].clone();

            if start.func == stop.func && pc < stop.pc {
                break;
            }

            match self.dispatch(opcode, InstructionPointer{func: func_pointer, pc: pc }) {
                DispatchResult::Next => {
                    pc += 1;
                }
                DispatchResult::Jump(new_pc) => {
                    if new_pc < start.pc {
                        return;
                    }
                    pc = new_pc;
                }
                DispatchResult::Call(func, new_pc) => {
                    func_pointer = func;
                    pc = new_pc;
                }
                DispatchResult::Stop => {
                    panic!("STOPPED");
                }
            }

        }
    }

    /// execute a linear trace - returns on guard failure
    pub fn run_trace(&mut self, trace: &[OpCode]) -> Guard {
        let mut pc: usize = 0;

        loop {
            if pc >= trace.len() {
                pc = 0;
            }

            let opcode = trace[pc].clone();
            match opcode {
                OpCode::Panic => panic!("assertion failed"),

                OpCode::Guard(guard)=> {
                    // we have to leave the boolean value on the stack
                    // so the blackhole interpreter can branch correctly
                    match self.peek_value() {
                        // guard success
                        R_BoxedValue::Bool(value) if value == guard.expected => {
                            // pop peeked value

                            self.stack.pop();
                        },

                        // guard failure
                        R_BoxedValue::Bool(_) => {
                            return guard;
                        }

                        // something completely wrong
                        val => panic!("expected bool, got {:?}", val),
                    }
                }

                OpCode::ConstValue(val) => {
                    self.stack.push(StackVal::Owned(val));
                }

                OpCode::Tuple(size) => self.o_tuple(size),
                OpCode::TupleInit(size) => self.o_tuple_init(size),
                OpCode::TupleGet(idx) => self.o_tuple_get(idx),
                OpCode::TupleSet(idx) => self.o_tuple_set(idx),

                // XXX: proper implementation of unsize
                OpCode::Unsize | OpCode::Use => {
                    let val = self.stack.pop().unwrap().into_owned();
                    self.stack.push(val);
                }

                OpCode::Ref => self.o_ref(),

                OpCode::Deref => self.o_deref(),

                OpCode::Load(local_index) => self.o_load(local_index),

                OpCode::Store(local_index) => self.o_store(local_index),

                // OpCode::Call => {
                //     // load and activate func
                //     func_pointer = self.o_call(func_pointer, pc);
                //     // jump to first instruction of function
                //     // continue is necessary because else pc += 1 would be executed
                //     pc = 0;
                //     continue;
                // }

                // OpCode::Static(static_idx) => {
                //     func_pointer = self.o_load_static(static_idx, func_pointer, pc);
                //     pc = 0;
                //     continue;
                // }

                // OpCode::Return => {
                //     if let Some(ret) = self.o_return() {
                //         func_pointer = ret.func;
                //         pc = ret.pc;
                //     } else {
                //         break;
                //     }
                // }

                OpCode::Skip(n) => {
                    pc += n;
                    continue;
                }
                OpCode::JumpBack(n) => {
                    pc -= n;
                    continue;
                }

                OpCode::SkipIf(n) => {
                    let val = self.pop_value();
                    if let R_BoxedValue::Bool(b) = val {
                        if b {
                            pc += n;
                            continue;
                        }
                    } else {
                        panic!("expected bool, git {:?}", val);
                    }
                }
                OpCode::JumpBackIf(n) => {
                    let val = self.pop_value();
                    if let R_BoxedValue::Bool(b) = val {
                        if b {
                            pc -= n;
                            continue;
                        }
                    } else {
                        panic!("expected bool, git {:?}", val);
                    }
                }

                OpCode::GetIndex => self.o_get_index(),
                OpCode::AssignIndex => self.o_assign_index(),

                OpCode::Array(size) => self.o_array(size),

                OpCode::Repeat(size) => self.o_repeat(size),

                OpCode::Len => self.o_len(),

                OpCode::BinOp(kind) => self.o_binop(kind),
                OpCode::CheckedBinOp(kind) => self.o_checked_binop(kind),

                OpCode::Not => self.o_not(),
                OpCode::Neg => unimplemented!(),
                OpCode::Noop => (),

                _ => {
                    println!("XXX: {:?}", opcode);
                    unimplemented!()
                }
            }

            pc += 1;
        }
    }

    pub fn stack_ptr(&self) -> usize {
        self.stack_frames.len() - 1
    }

    pub fn active_frame(&self) -> &CallFrame {
        self.stack_frames.last().unwrap()
    }

    pub fn o_load(&mut self, local_idx: usize) {
        let cell_ptr = self.active_frame().locals[local_idx].clone();
        self.stack.push(StackVal::Ref(cell_ptr))
    }

    pub fn o_store(&mut self, local_idx: usize) {
        let val = self.stack.pop().unwrap();
        let mut cell = self.active_frame().locals[local_idx].borrow_mut();
        *cell = val.unwrap_value();
    }

    pub fn o_ref(&mut self) {
        let addr = self.stack.pop().unwrap().into_pointer();
        self.stack.push(addr);
    }

    pub fn o_deref(&mut self) {
        let address = self.stack.pop().unwrap().deref();
        self.stack.push(address);
    }

    pub fn o_call(&mut self, cur_func: usize, cur_pc: usize) -> usize {
        if let R_BoxedValue::Func(idx) = self.stack.pop().unwrap().into_owned().unwrap_value() {
            let func = &self.program[idx];
            let return_addr = InstructionPointer {
                func: cur_func,
                pc: cur_pc,
            };
            let mut frame = CallFrame::new(Some(return_addr), func.1);
            for idx in (0..func.0).rev() {
                frame.locals[idx] = self.stack.pop().unwrap().into_cell().unwrap_cell();
            }
            self.stack_frames.push(frame);
            idx
        } else {
            panic!("expected func");
        }
    }

    pub fn o_load_static(&mut self, static_idx: usize, cur_func: usize, cur_pc: usize) -> usize {
        let func = &self.program[static_idx];
        let return_addr = InstructionPointer {
            func: cur_func,
            pc: cur_pc,
        };
        let mut frame = CallFrame::new(Some(return_addr), 0);
        self.stack_frames.push(frame);
        static_idx
    }

    pub fn o_return(&mut self) -> Option<InstructionPointer> {
        match self.stack_frames.pop() {
            Some(frame) => frame.return_addr,
            None => None,
        }
    }

    pub fn o_tuple(&mut self, size: usize) {
        let mut tuple = R_Struct::tuple(size);
        self.stack.push(StackVal::Owned(R_BoxedValue::Struct(tuple)));
    }

    pub fn o_tuple_init(&mut self, idx: usize) {
        let val = self.pop_value();
        if let R_BoxedValue::Struct(ref mut tuple) = self.stack
            .last()
            .unwrap()
            .clone()
            .unwrap_value() {
            tuple.set(idx, val);
        } else {
            panic!("tuple init");
        }
    }

    pub fn o_tuple_set(&mut self, idx: usize) {
        let boxed_tuple = self.pop_value();
        let val = self.pop_value();

        if let R_BoxedValue::Struct(mut tuple) = boxed_tuple {
            tuple.set(idx, val);
        } else {
            panic!("expected struct, got {:?}", boxed_tuple);
        }
    }

    pub fn o_tuple_get(&mut self, idx: usize) {
        let val = self.pop_value();
        if let R_BoxedValue::Struct(r_struct) = val {
            let ptr = r_struct.data[idx].clone();
            self.stack.push(StackVal::Ref(ptr));
        } else {
            panic!("expected struct got {:?}", val);
        }
    }

    pub fn load_const(&mut self, idx: usize) -> R_BoxedValue {
        let func = &self.program[idx];
        if let OpCode::ConstValue(ref val) = func.2[0] {
            val.clone()
        } else {
            panic!("expected const");
        }
    }

    pub fn peek_value(&mut self) -> R_BoxedValue {
        let val = self.stack.last().unwrap().clone().into_owned().unwrap_value();
        if let R_BoxedValue::Static(def_id) = val {
            self.load_const(def_id)
        } else {
            val
        }
    }


    pub fn pop_value(&mut self) -> R_BoxedValue {
        let val = self.stack.pop().unwrap().into_owned().unwrap_value();
        if let R_BoxedValue::Static(def_id) = val {
            self.load_const(def_id)
        } else {
            val
        }
    }

    pub fn o_binop(&mut self, kind: BinOp) {
        let val = self._do_binop(kind);
        self.stack.push(StackVal::Owned(val));
    }

    pub fn o_checked_binop(&mut self, kind: BinOp) {
        // TODO: actually check binops
        let mut tuple = R_Struct::tuple(2);
        *tuple.data[0].borrow_mut() = self._do_binop(kind);
        // false == no error
        *tuple.data[1].borrow_mut() = R_BoxedValue::Bool(false);
        self.stack.push(StackVal::Owned(R_BoxedValue::Struct(tuple)));
    }

    fn _do_binop(&mut self, kind: BinOp) -> R_BoxedValue {

        use core::objects::R_BoxedValue::*;
        use bc::bytecode::BinOp::*;

        let right = self.pop_value();
        let left = self.pop_value();

        debug!("#EX2 left: {:?}, right: {:?} ", left, right);
        // copied from miri
        macro_rules! int_binops {
            ($v:ident, $l:ident, $r:ident) => ({
                match kind {
                    Add    => $v($l + $r),
                    Sub    => $v($l - $r),
                    Mul    => $v($l * $r),
                    Div    => $v($l / $r),
                    Rem    => $v($l % $r),
                    BitXor => $v($l ^ $r),
                    BitAnd => $v($l & $r),
                    BitOr  => $v($l | $r),

                    // TODO(solson): Can have differently-typed RHS.
                    Shl => $v($l << $r),
                    Shr => $v($l >> $r),

                    Eq => Bool($l == $r),
                    Ne => Bool($l != $r),
                    Lt => Bool($l < $r),
                    Le => Bool($l <= $r),
                    Gt => Bool($l > $r),
                    Ge => Bool($l >= $r),
                }
            })
        }


        match (left, right) {
            (I64(l), I64(r)) => int_binops!(I64, l, r),
            (U64(l), U64(r)) => int_binops!(U64, l, r),
            (Usize(l), Usize(r)) => int_binops!(Usize, l, r),

            // copied from miri
            (Bool(l), Bool(r)) => {
                Bool(match kind {
                    Eq => l == r,
                    Ne => l != r,
                    Lt => l < r,
                    Le => l <= r,
                    Gt => l > r,
                    Ge => l >= r,
                    BitOr => l | r,
                    BitXor => l ^ r,
                    BitAnd => l & r,
                    Add | Sub | Mul | Div | Rem | Shl | Shr => {
                        panic!("invalid binary operation on booleans: {:?}", kind)
                    }
                })
            }

            (l, r) => {
                println!("{:?} {:?}", l, r);
                unimplemented!();
            }
        }
    }

    pub fn o_not(&mut self) {
        if let R_BoxedValue::Bool(boolean) = self.pop_value() {
            self.stack.push(StackVal::Owned(R_BoxedValue::Bool(!boolean)));
        } else {
            panic!("expected bool");
        }
    }

    pub fn o_get_index(&mut self) {
        let target = self.pop_value();
        let index = self.pop_value();
        if let (R_BoxedValue::Struct(mut r_struct), R_BoxedValue::Usize(idx)) = (target, index) {
            let val = r_struct.get(idx);
            self.stack.push(StackVal::Ref(val));
        } else {
            panic!("error");
        }
    }

    pub fn o_assign_index(&mut self) {
        let target = self.pop_value();
        let index = self.pop_value();
        let val = self.pop_value();
        if let (R_BoxedValue::Struct(mut r_struct), R_BoxedValue::Usize(idx)) = (target, index) {
            r_struct.set(idx, val);
        } else {
            panic!("error");
        }
    }

    pub fn o_array(&mut self, size: usize) {
        let mut obj = R_Struct::with_size(size);
        for idx in (0..size).rev() {
            let val = self.pop_value();
            obj.set(idx, val.clone());
        }
        self.stack.push(StackVal::Owned(R_BoxedValue::Struct(obj)));
    }

    pub fn o_repeat(&mut self, size: usize) {
        let val = self.pop_value();

        let mut obj = R_Struct::with_size(size);
        for idx in 0..size {
            obj.set(idx, val.clone());
        }

        self.stack.push(StackVal::Owned(R_BoxedValue::Struct(obj)));
    }

    pub fn o_len(&mut self) {
        let x = self.pop_value();
        match x {
            R_BoxedValue::Struct(s) => {
                self.stack.push(StackVal::Owned(R_BoxedValue::Usize(s.data.len())));
            }
            R_BoxedValue::Array(inner_vec) => {
                self.stack.push(StackVal::Owned(R_BoxedValue::Usize(inner_vec.len())));
            }
            _ => panic!("can't get len of {:?}", x),
        }
    }
}
