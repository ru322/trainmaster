use super::*;

#[test]
fn pause_presets_follow_a4() {
    assert_eq!(
        (Pause::STANDARD.start_s, Pause::STANDARD.unlock_s),
        (90, 15)
    );
    assert_eq!((Pause::LONG.start_s, Pause::LONG.unlock_s), (120, 20));
    assert_eq!((Pause::NONE.start_s, Pause::NONE.unlock_s), (0, 0));
    for p in Pause::ALL {
        assert_eq!(Pause::parse(&p.name()), Some(p));
    }
    let custom = Pause {
        start_s: 70,
        unlock_s: 13,
    };
    assert_eq!(custom.name(), "70+13");
    assert_eq!(Pause::parse("70+13"), Some(custom));
    assert_eq!(Pause::parse("x"), None);
}

#[test]
fn build_times_follow_a3() {
    assert_eq!((CREATE_S, EXTEND_S), (15, 8));
}

#[test]
fn idle_windows_parse_minutes() {
    assert_eq!(
        Idle::parse_minutes("90-690,780-"),
        Some(vec![
            Idle {
                start_s: 5400,
                end_s: 41_400
            },
            Idle::after(46_800)
        ])
    );
    assert_eq!(Idle::parse_minutes("5-"), Some(vec![Idle::after(300)]));
    for bad in ["", "5", "9-3", "3-3", "a-4", "1-2,x"] {
        assert_eq!(Idle::parse_minutes(bad), None, "{bad}");
    }
}

#[test]
fn idle_window_is_half_open_on_wall_ticks() {
    let mut sc = Scenario::new(Player::B, Pause::NONE);
    assert!(!sc.is_idle(0));
    sc.idle = vec![
        Idle {
            start_s: 2,
            end_s: 3,
        },
        Idle::after(10),
    ];
    assert!(!sc.is_idle(2 * TPS - 1));
    assert!(sc.is_idle(2 * TPS));
    assert!(sc.is_idle(3 * TPS - 1));
    assert!(!sc.is_idle(3 * TPS));
    assert!(sc.is_idle(10 * TPS));
    assert!(sc.is_idle(u64::MAX));
}

#[test]
fn scenario_defaults_follow_sim_d_27() {
    let sc = Scenario::new(Player::A, Pause::LONG);
    assert_eq!((sc.route, sc.save_s), (Route::Far, 300));
    assert!(sc.idle.is_empty() && sc.session_s.is_none() && !sc.stop_at_goal);
    assert_eq!(sc.with_route(Route::Near).route, Route::Near);
}

#[test]
fn only_save_counts_as_waiting_for_track() {
    assert_eq!(saved_secs(Decision::Save, Player::C), 8);
    assert_eq!(saved_secs(Decision::Save, Player::A), 2);
    assert_eq!(saved_secs(Decision::Wait, Player::B), 0);
    assert_eq!(saved_secs(Decision::BuyLineSlots, Player::B), 0);
}
