use rubik::{Axis, ByteArray, Cube, Move, TurnAmount};

fn main() {
    let mut cube = Cube::<ByteArray>::new_solved(3);
    cube.apply_move(Move::new(Axis::Z, 2, TurnAmount::Cw));

    println!("{cube}");
    println!("{}", cube.preview_net_string(3));
}
