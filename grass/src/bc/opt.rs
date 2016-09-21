
use std::collections::{HashMap, HashSet};

use super::bytecode::OpCode;


pub fn eliminate_unused_vars(stream: &Vec<OpCode>) -> Vec<OpCode> {
    let mut active: HashSet<usize> = HashSet::new();
    let mut active_cnt: HashMap<usize, usize> = HashMap::new();
    let mut new = Vec::new();

    // let mut dirty = false;

    for oc in stream.iter().rev() {
        match *oc {
            OpCode::Load(var) => {
                let count = active_cnt.entry(var).or_insert(0);
                *count += 1;

                new.push(oc.clone());
            },

            OpCode::Store(var) => {
                {
                    let count = active_cnt.entry(var).or_insert(0);

                    // var is not used
                    if *count == 0 {
                        // dirty = true;
                        new.push(OpCode::Pop);
                        continue;
                    } else if let Some(&OpCode::Load(load_var)) = new.last() {
                        if load_var == var && *count == 1 {
                            new.pop().unwrap();
                            continue;
                        }
                    }
                    // this var is used somewhere
                    active.insert(var);
                    new.push(oc.clone());
                }

                // reset counter for `Load`s
                active_cnt.insert(var, 0);
            },

            OpCode::Tuple(0) => {
                if let Some(&OpCode::Pop) = new.last() {
                    new.pop().unwrap();
                } else {
                    new.push(oc.clone());
                }
            },

            _ => new.push(oc.clone()),
        }
    }

    new
}