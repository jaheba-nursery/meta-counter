
extern crate grass;
use grass::driver;

mod this;


const DEC: usize = 0;
const REP: usize = 1;

fn main() {
    let mut my_driver = driver::Driver::default();

    let program = [DEC, REP];

    let mut cell = 10;
    let mut pc = 0;

    loop {
        pc = my_driver.merge_point(this::PROGRAM, this::IDX, &program, pc, &mut cell);

        if pc >= 2 {
            break;
        }

        let opcode = program[pc];

        if opcode == DEC {
            cell -= 1;
        } else if opcode == REP && cell > 0 {
            pc -= 1;
            continue;
        }
        pc += 1;
    }

    println!("{:?}", cell);
}