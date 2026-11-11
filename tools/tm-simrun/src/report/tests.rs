use super::*;

fn sample() -> Report {
    let mut m = Milestones::new(16);
    m.level[1] = Some(3600);
    m.level[4] = Some(3600 * 6 + 1800);
    m.level[16] = Some(36_000);
    m.all_features = m.level[4];
    m.all_params = Some(30_000);
    m.goal = Some(40_000);
    Report {
        seed: Some(42),
        player: Player::B,
        pause: Pause::STANDARD,
        route: Route::Near,
        end_wall: 108_000,
        end_level: 16,
        milestones: m,
        track_tiles: 57,
        track_spent: 12345.0,
        saved_s: 90,
        builds: vec![BuildRecord {
            wall: 7200,
            level: 1,
            new_tiles: 8,
            cost: 1.0e4,
            income: 40.0,
        }],
        reach: vec![
            Reach {
                wall: 90 * 3600,
                level: 14,
                stations: 17,
                unit: 10,
            },
            Reach {
                wall: 180 * 3600,
                level: 17,
                stations: 20,
                unit: 12,
            },
        ],
        game: Some(GameStats {
            routes_recomputed: 3,
            node_counts: BTreeMap::from([(8, 1), (62, 2)]),
            recompute_max_per_10s: 2,
            ledger: PaxLedgerView {
                spawned: 110,
                capacity_loss: 7,
                delivered: 80,
                waiting_cap_loss: 20,
                waiting: 6,
                onboard: 4,
            },
            waiting_peak_ratio: 0.5,
            terminal_cv: None,
            income_ratio: Some(0.25),
            rejected: 0,
            rec_checked: 9,
            rec_mismatch: 1,
            first_mismatch: None,
        }),
    }
}

#[test]
fn minutes_round_to_hundredths() {
    assert_eq!(minutes(0), "0.00");
    assert_eq!(minutes(3600), "1.00");
    assert_eq!(minutes(17), "0.00");
    assert_eq!(minutes(18), "0.01");
    assert_eq!(minutes(108_000), "30.00");
    assert_eq!(minutes(40_000), "11.11");
}

#[test]
fn header_lists_every_company_level() {
    let h = csv_header(3);
    assert!(h.starts_with("seed,player,pause,route,lv1,lv2,lv3,all_features,"));
    assert!(h.contains(",end_level,track_tiles,track_spent,builds,saved_min,routes_recomputed,"));
    assert!(h.ends_with(",rec_mismatch,reach"));
    assert_eq!(
        csv_header(24)
            .split(',')
            .filter(|c| c.starts_with("lv"))
            .count(),
        24
    );
}

#[test]
fn csv_row_lists_columns_in_header_order() {
    let row = sample().csv_row();
    let lv = "1.00,,,6.50,,,,,,,,,,,,10.00";
    assert_eq!(
        row,
        format!(
            "42,b,standard,near,{lv},6.50,8.33,,,11.11,30.00,16,57,1.234e4,1,1.50,\
3,8,62,2,80,20,0.250000,0.5000,,0.250,0,9,1,90=14/17/10;180=17/20/12"
        )
    );
    assert_eq!(row.split(',').count(), csv_header(16).split(',').count());
}

#[test]
fn flow_rows_leave_game_columns_empty() {
    let mut r = sample();
    r.seed = None;
    r.game = None;
    r.reach.clear();
    let row = r.csv_row();
    assert!(row.starts_with("flow,b,standard,near,"));
    assert!(row.ends_with(",30.00,16,57,1.234e4,1,1.50,,,,,,,,,,,,,,"));
    assert_eq!(row.split(',').count(), csv_header(16).split(',').count());
}

#[test]
fn build_rows_give_cost_in_seconds_of_income() {
    let mut r = sample();
    assert_eq!(
        r.build_rows(),
        ["42,b,near,2.00,1,8,1.000e4,4.000e1,250.00"]
    );
    r.builds[0].income = 0.0;
    assert_eq!(r.build_rows(), ["42,b,near,2.00,1,8,1.000e4,0.000e0,"]);
    assert_eq!(BUILDS_HEADER.split(',').count(), 9);
}

#[test]
fn features_and_goal_deadline() {
    let r = sample();
    assert!(r.features_and_goal_within(40_000));
    assert!(!r.features_and_goal_within(39_999));
    let mut no_goal = r.clone();
    no_goal.milestones.goal = None;
    assert!(!no_goal.features_and_goal_within(u64::MAX));
}

#[test]
fn mark_keeps_first_time() {
    let mut slot = None;
    Milestones::mark(&mut slot, 5);
    Milestones::mark(&mut slot, 3);
    assert_eq!(slot, Some(5));
}
