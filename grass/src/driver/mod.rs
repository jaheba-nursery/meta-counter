

mod meta;

use std::rc::Rc;
use std::collections::BTreeMap;


use bc::bytecode::{OpCode, Guard};
use core::objects::{CallFrame, InstructionPointer, R_BoxedValue, R_Struct};

#[derive(Default)]
pub struct Driver {
    tracer: Tracer,
}

// TODO: pass &mut Tape to merge_point

type Program = [(usize, usize, &'static [OpCode])];

impl Driver {
    pub fn merge_point<'a>(&mut self,
                           program: &Program,
                           (fn_idx, oc_idx): (usize, usize),
                           user_program: &[usize],
                           pc: usize,
                           cell: &'a mut usize)
                           -> usize {

        let res = self.tracer.handle_mergepoint(pc as u64);

        match res {
            MergePointResult::StartTrace => {
                let func = &program[fn_idx];

                let mut s = R_Struct::with_size(user_program.len());
                for (i, uoc) in user_program.iter().enumerate() {
                    s.set(i, R_BoxedValue::Usize(*uoc));
                }

                let mut frame = CallFrame::new(None, func.1);
                *frame.locals[1].borrow_mut() = R_BoxedValue::Struct(s);
                *frame.locals[2].borrow_mut() = R_BoxedValue::Usize(*cell);
                *frame.locals[3].borrow_mut() = R_BoxedValue::Usize(pc);
                let prog = program.iter().map(|&(fni, pc, ocs)| (fni, pc, ocs.to_vec())).collect();

                let mut interp = meta::interp::Interpreter::new(&prog);
                interp.stack_frames.push(frame);
                interp.run(Some(&mut self.tracer), fn_idx, oc_idx);
                self.tracer.finish_trace(pc as u64);

                let frame = &interp.stack_frames[0];

                // retrieve state
                let boxed_cell  = (*frame.locals[2].borrow()).clone();
                if let R_BoxedValue::Usize(content) = boxed_cell {
                    *cell = content;
                }

                // retrieve pc
                let boxed_pc = (*frame.locals[3].borrow()).clone();

                if let R_BoxedValue::Usize(ref new_pc) = boxed_pc {
                    new_pc.clone()
                } else {
                    panic!("");
                }
            }

            MergePointResult::Trace(trace) => {
                let func = &program[fn_idx];

                let mut s = R_Struct::with_size(user_program.len());
                for (i, uoc) in user_program.iter().enumerate() {
                    s.set(i, R_BoxedValue::Usize(*uoc));
                }

                let mut frame = CallFrame::new(None, func.1);
                *frame.locals[1].borrow_mut() = R_BoxedValue::Struct(s);
                *frame.locals[2].borrow_mut() = R_BoxedValue::Usize(*cell);
                *frame.locals[3].borrow_mut() = R_BoxedValue::Usize(pc);
                let prog = program.iter().map(|&(fni, pc, ocs)| (fni, pc, ocs.to_vec())).collect();

                let mut interp = meta::interp::Interpreter::new(&prog);
                interp.stack_frames.push(frame);

                let inst = interp.run_trace(&*trace);
                // some guard failed
                // should we side trace?
                println!("inst: {:?}, pc: {}", inst.pc, pc);


                // blackhole?
                interp.run(None, fn_idx, inst.pc);

                let frame = &interp.stack_frames[0];

                // retrieve state
                let boxed_cell  = (*frame.locals[2].borrow()).clone();
                if let R_BoxedValue::Usize(content) = boxed_cell {
                    *cell = content;
                }

                // retrieve pc
                let boxed_pc = (*frame.locals[3].borrow()).clone();

                if let R_BoxedValue::Usize(ref new_pc) = boxed_pc {

                    new_pc.clone()
                } else {
                    panic!("");
                }


            }

            MergePointResult::None => pc,
        }
    }
}


type HashValue = u64;
const HOT_LOOP_THRESHOLD: usize = 2;

// glorified Option
#[derive(Clone, Debug)]
pub enum MergePointResult {
    Trace(Rc<Vec<OpCode>>),
    StartTrace,
    None,
}

#[derive(Default)]
pub struct Tracer {
    /// counter for program positions
    counter: BTreeMap<HashValue, usize>,
    traces: BTreeMap<HashValue, Rc<Vec<OpCode>>>,
    loop_start: HashValue,

    active: Option<Vec<OpCode>>,
}

impl Tracer {
    pub fn handle_mergepoint(&mut self, key: HashValue) -> MergePointResult {

        if self.traces.contains_key(&key) {
            return MergePointResult::Trace(self.traces.get(&key).unwrap().clone());
        }
        // increase counter for program position
        else if self.active.is_none() {
            let count = {
                let count = self.counter.entry(key).or_insert(0);
                *count += 1;
                *count
            };

            if count > HOT_LOOP_THRESHOLD {
                self.active = Some(Vec::new());
                self.counter.clear();
                self.loop_start = key;
                return MergePointResult::StartTrace;
            }
        }
        // close the loop
        else if key == self.loop_start {
            self.finish_trace(key);
        }

        MergePointResult::None
    }

    pub fn finish_trace(&mut self, key: HashValue) {
        let active = self.active.take().unwrap();
        self.traces.insert(key, Rc::new(active));
    }

    pub fn trace_opcode(&mut self, opcode: &OpCode, pos: InstructionPointer) {
        let oc = match *opcode {
            OpCode::Skip(_) { return; },

            OpCode::JumpBack(_) => {
                return;
            }

            OpCode::SkipIf(_) |
            OpCode::JumpBackIf(_) => {
                let guard = Guard {
                    expected: true,
                    recovery: pos,
                };
                OpCode::Guard(guard)
            }

            _ => opcode.clone(),
        };

        // self.active.map(|ocs| ocs.push(oc));
        self.active.as_mut().unwrap().push(oc);
    }
}
