use rubik::{Axis, ByteArray, Cube, Move, TurnAmount};

fn main() {
    let mut cube = Cube::<ByteArray>::new_solved(9);
    cube.apply_move(Move::new(Axis::Z, 2, TurnAmount::Cw));

    println!("{cube}");
    println!("{}", cube.net_string());
}
