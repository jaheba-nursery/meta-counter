
extern crate grass;
use grass::driver;

mod this;


const DEC: usize = 0;
const REP: usize = 1;
const INC: usize = 2;

fn main() {
    let mut my_driver = driver::Driver::default();

    let program = [DEC, REP, INC, INC, INC, INC, INC, DEC, REP, INC];

    let mut cell = 10;
    let mut pc = 0;

    loop {
        pc = my_driver.merge_point(this::PROGRAM, this::IDX, &program, pc, &mut cell);

        if pc >= 10 {
            break;
        }

        let opcode = program[pc];

        if opcode == DEC {
            cell -= 1;
            println!("DEC");
        } else if opcode == REP && cell > 0 {
            pc -= 1;
            continue;
        } else if opcode == INC {
            cell += 1;
            println!("INC");
        }
        pc += 1;
    }

    // println!("{:?}", cell);
}