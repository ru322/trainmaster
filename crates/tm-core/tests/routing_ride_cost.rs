//! 走行辺のコストのゴールデン(12 §8.5 G-01、§3.4 の表)。

use tm_core::{Permille, SpeedMilli, Tiles, ride_cost};

#[test]
fn g01_ride_cost_golden() {
    let table = [
        (6, 1000, 1000, 800),
        (6, 1000, 3000, 267),
        (6, 6000, 1000, 300),
        (1, 1000, 1600, 188),
        (1, 6000, 3000, 72),
        (3, 1500, 1200, 333),
        (576, 1000, 1000, 57800),
    ];
    for (d, v, a, want) in table {
        let got = ride_cost(Tiles(d), SpeedMilli(v), Permille(a), 2);
        assert_eq!(got.0, want, "d={d} v={v} a={a}");
    }
}

#[test]
fn ride_cost_is_at_least_one() {
    assert_eq!(
        ride_cost(Tiles(0), SpeedMilli(6000), Permille(3000), 0).0,
        1
    );
}
