use tm_platform_api::{
    AudioStats, DrawList, LOGICAL_HEIGHT, LOGICAL_WIDTH, MonoTime, PlatformStats, SpriteRect,
};

#[test]
fn logical_resolution_is_640_by_360() {
    assert_eq!((LOGICAL_WIDTH, LOGICAL_HEIGHT), (640, 360));
}

#[test]
fn mono_time_saturates() {
    assert_eq!(MonoTime(10).saturating_since(MonoTime(4)), 6);
    assert_eq!(MonoTime(4).saturating_since(MonoTime(10)), 0);
}

#[test]
fn defaults_are_empty() {
    assert!(DrawList::default().cmds.is_empty());
    assert_eq!(PlatformStats::default().audio, AudioStats::default());
    assert!(!AudioStats::default().enabled);
    assert_eq!(SpriteRect::ZERO.w, 0);
}
