use rubik::{Angle, Axis, ByteArray, Cube, Move};

fn main() {
    let mut cube = Cube::<ByteArray>::new_solved(4);
    cube.apply_move(Move::new(Axis::Z, 0, Angle::Positive));

    println!("{cube}");
    println!("{}", cube.net_string());
}
