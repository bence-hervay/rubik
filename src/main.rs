use rubik::{Axis, Byte, Cube, Move, MoveAngle};

fn main() {
    let mut cube = Cube::<Byte>::new_solved(4);
    cube.apply_move(Move::new(Axis::Z, 0, MoveAngle::Positive));

    println!("{cube}");
    println!("{}", cube.net_string());
}
