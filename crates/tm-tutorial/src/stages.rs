//! 段階の表(13 §6.2、§6.3)と、題・ページを引く関数。

use tm_assets::text::{TextId, txt};
use tm_core::{ParamKey, UpgradeScope};

use crate::anchor::{MenuAnchor, SettingsItem, UiAnchor};
use crate::directive::TutorialStageId;
use crate::location::BuildKind;
use crate::permits::PermitRule;

/// 段階 1 つ分の定義。
pub(crate) struct StageDef {
    pub(crate) id: TutorialStageId,
    pub(crate) title: TextId,
    pub(crate) trigger: Trigger,
    /// trigger を待つ間の止めない案内(#4、#6)。
    pub(crate) pre_guide: Option<GuideDef>,
    pub(crate) pages: &'static [TextId],
    /// `pages` と同じ長さ。
    pub(crate) highlights: &'static [Hl],
    pub(crate) guides: &'static [GuideDef],
    /// 次の段階の trigger が成り立ったら案内を打ち切ってよい。
    pub(crate) supersedable: bool,
    pub(crate) skip: SkipCond,
    pub(crate) permit_wait: PermitRule,
    pub(crate) permit_guide: PermitRule,
}

/// 説明の後(か trigger を待つ間)の案内。
#[derive(Clone, Copy, Debug)]
pub(crate) struct GuideDef {
    pub(crate) text: TextId,
    pub(crate) target: GuideTarget,
    pub(crate) done: DoneCond,
    /// `done` のほかに「済んだ扱い」にする条件。
    pub(crate) already: Already,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    Start,
    AfterPrev,
    FirstIncome,
    CanAfford(ParamKey, UpgradeScope),
    PurchaseCount(u32),
    GaugeOrCompany { percent: u8, level: u8 },
    CompanyLevel(u8),
    Goal,
}

/// 光らせる場所。`Fixed` 以外は `observe_view` で決まる。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hl {
    None,
    Fixed(UiAnchor),
    UnconnectedStation,
    NewestStation,
    FirstTrain,
    FirstStation,
}

/// 案内が指す場所。`Buy` と `Build` は UI の現在地で変わる(13 §6.5)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuideTarget {
    Hl(Hl),
    Buy(ParamKey),
    Build(BuildKind),
    BuyMax,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DoneCond {
    CursorOnStation,
    LineCreated,
    LineExtended,
    Bought(ParamKey),
    BoughtMax,
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Already {
    Never,
    FreeLineSlot,
    LinesAtLeast(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SkipCond {
    Never,
    AnyLine,
    AllStationsConnected,
    AlreadyBought(ParamKey),
}

const fn menu(m: MenuAnchor) -> Hl {
    Hl::Fixed(UiAnchor::Menu(m))
}

const fn item(p: ParamKey) -> Hl {
    menu(MenuAnchor::UpgradeItem(p))
}

const fn guide(text: TextId, target: GuideTarget, done: DoneCond) -> GuideDef {
    GuideDef {
        text,
        target,
        done,
        already: Already::Never,
    }
}

const fn buy(text: TextId, p: ParamKey) -> GuideDef {
    guide(text, GuideTarget::Buy(p), DoneCond::Bought(p))
}

/// 本線の段階(#1〜#12、#14)。この順に進む。
pub(crate) static MAIN_STAGES: [StageDef; 13] = [
    StageDef {
        id: TutorialStageId(1),
        title: txt::TUT_S01_TITLE,
        trigger: Trigger::Start,
        pre_guide: None,
        pages: &[txt::TUT_S01_P1, txt::TUT_S01_P2],
        highlights: &[Hl::None, Hl::None],
        guides: &[],
        supersedable: false,
        skip: SkipCond::Never,
        permit_wait: PermitRule::Nothing,
        permit_guide: PermitRule::Nothing,
    },
    StageDef {
        id: TutorialStageId(2),
        title: txt::TUT_S02_TITLE,
        trigger: Trigger::AfterPrev,
        pre_guide: None,
        pages: &[txt::TUT_S02_P1],
        highlights: &[Hl::FirstStation],
        guides: &[guide(
            txt::TUT_S02_G1,
            GuideTarget::Hl(Hl::FirstStation),
            DoneCond::CursorOnStation,
        )],
        supersedable: false,
        skip: SkipCond::Never,
        permit_wait: PermitRule::Nothing,
        permit_guide: PermitRule::Nothing,
    },
    StageDef {
        id: TutorialStageId(3),
        title: txt::TUT_S03_TITLE,
        trigger: Trigger::AfterPrev,
        pre_guide: None,
        pages: &[txt::TUT_S03_P1, txt::TUT_S03_P2, txt::TUT_S03_P3],
        highlights: &[Hl::None, menu(MenuAnchor::BuildMode), Hl::None],
        guides: &[guide(
            txt::TUT_S03_G1,
            GuideTarget::Build(BuildKind::Create),
            DoneCond::LineCreated,
        )],
        supersedable: false,
        skip: SkipCond::AnyLine,
        permit_wait: PermitRule::Nothing,
        permit_guide: PermitRule::CreateLineOf(2),
    },
    StageDef {
        id: TutorialStageId(4),
        title: txt::TUT_S04_TITLE,
        trigger: Trigger::FirstIncome,
        pre_guide: Some(guide(
            txt::TUT_S04_W,
            GuideTarget::Hl(Hl::FirstTrain),
            DoneCond::Never,
        )),
        pages: &[txt::TUT_S04_P1],
        highlights: &[Hl::Fixed(UiAnchor::MoneyDisplay)],
        guides: &[],
        supersedable: false,
        skip: SkipCond::Never,
        permit_wait: PermitRule::CreateLineOfWhileNoLine(2),
        permit_guide: PermitRule::Nothing,
    },
    StageDef {
        id: TutorialStageId(5),
        title: txt::TUT_S05_TITLE,
        trigger: Trigger::AfterPrev,
        pre_guide: None,
        pages: &[txt::TUT_S05_P1, txt::TUT_S05_P2],
        highlights: &[Hl::UnconnectedStation, menu(MenuAnchor::BuildMode)],
        guides: &[guide(
            txt::TUT_S05_G1,
            GuideTarget::Build(BuildKind::Extend),
            DoneCond::LineExtended,
        )],
        supersedable: false,
        skip: SkipCond::AllStationsConnected,
        permit_wait: PermitRule::Nothing,
        permit_guide: PermitRule::ExtendOnly,
    },
    StageDef {
        id: TutorialStageId(6),
        title: txt::TUT_S06_TITLE,
        trigger: Trigger::CanAfford(ParamKey::S2Fare, UpgradeScope::AllStations),
        pre_guide: Some(guide(
            txt::TUT_S06_W,
            GuideTarget::Hl(Hl::Fixed(UiAnchor::MoneyDisplay)),
            DoneCond::Never,
        )),
        pages: &[txt::TUT_S06_P1, txt::TUT_S06_P2],
        highlights: &[item(ParamKey::S2Fare), menu(MenuAnchor::ScopeToggle)],
        guides: &[buy(txt::TUT_S06_G1, ParamKey::S2Fare)],
        supersedable: false,
        skip: SkipCond::AlreadyBought(ParamKey::S2Fare),
        permit_wait: PermitRule::Nothing,
        permit_guide: PermitRule::LevelUpOf(ParamKey::S2Fare),
    },
    StageDef {
        id: TutorialStageId(7),
        title: txt::TUT_S07_TITLE,
        trigger: Trigger::PurchaseCount(2),
        pre_guide: None,
        pages: &[txt::TUT_S07_P1, txt::TUT_S07_P2],
        highlights: &[
            menu(MenuAnchor::BuyMaxHint),
            Hl::Fixed(UiAnchor::RecommendMark),
        ],
        guides: &[guide(
            txt::TUT_S07_G1,
            GuideTarget::BuyMax,
            DoneCond::BoughtMax,
        )],
        supersedable: true,
        skip: SkipCond::Never,
        permit_wait: PermitRule::All,
        permit_guide: PermitRule::All,
    },
    StageDef {
        id: TutorialStageId(8),
        title: txt::TUT_S08_TITLE,
        trigger: Trigger::GaugeOrCompany {
            percent: 50,
            level: 1,
        },
        pre_guide: None,
        pages: &[txt::TUT_S08_P1],
        highlights: &[Hl::Fixed(UiAnchor::CompanyGauge)],
        guides: &[],
        supersedable: false,
        skip: SkipCond::Never,
        permit_wait: PermitRule::All,
        permit_guide: PermitRule::All,
    },
    StageDef {
        id: TutorialStageId(9),
        title: txt::TUT_S09_TITLE,
        trigger: Trigger::CompanyLevel(1),
        pre_guide: None,
        pages: &[
            txt::TUT_S09_P1,
            txt::TUT_S09_P2,
            txt::TUT_S09_P3,
            txt::TUT_S09_P4,
        ],
        highlights: &[
            Hl::NewestStation,
            item(ParamKey::R2LineSlots),
            Hl::None,
            Hl::Fixed(UiAnchor::MoneyDisplay),
        ],
        guides: &[
            GuideDef {
                already: Already::FreeLineSlot,
                ..buy(txt::TUT_S09_G1, ParamKey::R2LineSlots)
            },
            GuideDef {
                text: txt::TUT_S09_G2,
                target: GuideTarget::Build(BuildKind::Create),
                done: DoneCond::LineCreated,
                already: Already::LinesAtLeast(2),
            },
        ],
        supersedable: true,
        skip: SkipCond::Never,
        permit_wait: PermitRule::All,
        permit_guide: PermitRule::All,
    },
    StageDef {
        id: TutorialStageId(10),
        title: txt::TUT_S10_TITLE,
        trigger: Trigger::CompanyLevel(2),
        pre_guide: None,
        pages: &[txt::TUT_S10_P1, txt::TUT_S10_P2],
        highlights: &[item(ParamKey::T5Cars), item(ParamKey::S4Platform)],
        guides: &[
            buy(txt::TUT_S10_G1, ParamKey::S4Platform),
            buy(txt::TUT_S10_G2, ParamKey::T5Cars),
        ],
        supersedable: true,
        skip: SkipCond::Never,
        permit_wait: PermitRule::All,
        permit_guide: PermitRule::All,
    },
    StageDef {
        id: TutorialStageId(11),
        title: txt::TUT_S11_TITLE,
        trigger: Trigger::CompanyLevel(3),
        pre_guide: None,
        pages: &[txt::TUT_S11_P1, txt::TUT_S11_P2],
        highlights: &[
            item(ParamKey::R1MaxTrains),
            item(ParamKey::R3Attractiveness),
        ],
        guides: &[
            buy(txt::TUT_S11_G1, ParamKey::R1MaxTrains),
            buy(txt::TUT_S11_G2, ParamKey::R3Attractiveness),
        ],
        supersedable: true,
        skip: SkipCond::Never,
        permit_wait: PermitRule::All,
        permit_guide: PermitRule::All,
    },
    StageDef {
        id: TutorialStageId(12),
        title: txt::TUT_S12_TITLE,
        trigger: Trigger::CompanyLevel(4),
        pre_guide: None,
        pages: &[txt::TUT_S12_P1, txt::TUT_S12_P2, txt::TUT_S12_P3],
        highlights: &[
            item(ParamKey::T3Boarding),
            item(ParamKey::T2Crowding),
            Hl::None,
        ],
        guides: &[
            buy(txt::TUT_S12_G1, ParamKey::T3Boarding),
            buy(txt::TUT_S12_G2, ParamKey::T2Crowding),
        ],
        supersedable: true,
        skip: SkipCond::Never,
        permit_wait: PermitRule::All,
        permit_guide: PermitRule::All,
    },
    StageDef {
        id: TutorialStageId(14),
        title: txt::TUT_S14_TITLE,
        trigger: Trigger::Goal,
        pre_guide: None,
        pages: &[txt::TUT_S14_P1],
        highlights: &[Hl::None],
        guides: &[],
        supersedable: false,
        skip: SkipCond::Never,
        permit_wait: PermitRule::All,
        permit_guide: PermitRule::All,
    },
];

/// #15(せってい の説明)。本線とは別の流れで、trigger と permits は使わない。
pub(crate) static SETTINGS_STAGE: StageDef = StageDef {
    id: TutorialStageId(15),
    title: txt::TUT_S15_TITLE,
    trigger: Trigger::Start,
    pre_guide: None,
    pages: &[txt::TUT_S15_P1, txt::TUT_S15_P2, txt::TUT_S15_P3],
    highlights: &[
        menu(MenuAnchor::SettingsItem(SettingsItem::SoundVolume)),
        menu(MenuAnchor::SettingsItem(SettingsItem::Reset)),
        menu(MenuAnchor::SettingsItem(SettingsItem::Quit)),
    ],
    guides: &[],
    supersedable: false,
    skip: SkipCond::Never,
    permit_wait: PermitRule::Nothing,
    permit_guide: PermitRule::Nothing,
};

fn all_stages() -> impl Iterator<Item = &'static StageDef> {
    MAIN_STAGES.iter().chain(core::iter::once(&SETTINGS_STAGE))
}

pub(crate) fn find_stage(stage: TutorialStageId) -> Option<&'static StageDef> {
    all_stages().find(|s| s.id == stage)
}

fn stage_def(stage: TutorialStageId) -> &'static StageDef {
    find_stage(stage).unwrap_or_else(|| panic!("段階 #{} は存在しない", stage.0))
}

/// 説明のページの文から (段階, ページ) を引く。
pub(crate) fn page_of(text: TextId) -> Option<(TutorialStageId, u8)> {
    all_stages().find_map(|s| {
        let i = s.pages.iter().position(|&p| p == text)?;
        Some((s.id, i as u8))
    })
}

/// 段階の題。`stage` は #1〜#12、#14、#15 のどれか(それ以外は panic)。
pub fn stage_title(stage: TutorialStageId) -> TextId {
    stage_def(stage).title
}

/// 段階の説明のページ(1..=4)。`stage` は #1〜#12、#14、#15 のどれか(それ以外は panic)。
pub fn stage_pages(stage: TutorialStageId) -> &'static [TextId] {
    stage_def(stage).pages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_1_to_12_14_15_in_order() {
        let ids: Vec<u8> = all_stages().map(|s| s.id.0).collect();
        assert_eq!(ids, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14, 15]);
    }

    #[test]
    fn pages_are_one_to_four_and_match_highlights() {
        for s in all_stages() {
            let n = stage_pages(s.id).len();
            assert!((1..=4).contains(&n), "#{} は {n} ページ", s.id.0);
            assert_eq!(s.highlights.len(), n, "#{}", s.id.0);
            assert!(s.guides.len() <= 2, "#{}", s.id.0);
        }
    }

    #[test]
    fn title_and_pages_are_looked_up_by_stage() {
        assert_eq!(stage_title(TutorialStageId(3)), txt::TUT_S03_TITLE);
        assert_eq!(stage_pages(TutorialStageId(1))[0], txt::TUT_S01_P1);
        assert_eq!(stage_title(TutorialStageId(15)), txt::TUT_S15_TITLE);
        assert_eq!(
            stage_pages(TutorialStageId(12)),
            [txt::TUT_S12_P1, txt::TUT_S12_P2, txt::TUT_S12_P3]
        );
    }

    #[test]
    fn page_of_finds_every_page_and_nothing_else() {
        for s in all_stages() {
            for (i, &p) in s.pages.iter().enumerate() {
                assert_eq!(page_of(p), Some((s.id, i as u8)));
            }
        }
        assert_eq!(page_of(txt::TUT_S09_G1), None);
        assert_eq!(page_of(txt::TUT_S01_TITLE), None);
        assert!(find_stage(TutorialStageId(13)).is_none());
    }

    #[test]
    fn stage_table_is_pinned_for_saves() {
        let got: Vec<(u8, usize, usize)> = all_stages()
            .map(|s| (s.id.0, s.pages.len(), s.guides.len()))
            .collect();
        assert_eq!(
            got,
            [
                (1, 2, 0),
                (2, 1, 1),
                (3, 3, 1),
                (4, 1, 0),
                (5, 2, 1),
                (6, 2, 1),
                (7, 2, 1),
                (8, 1, 0),
                (9, 4, 2),
                (10, 2, 2),
                (11, 2, 2),
                (12, 3, 2),
                (14, 1, 0),
                (15, 3, 0),
            ],
            "段階表を変えたら tm-app の SAVE_FORMAT_VERSION を上げて、この表も直す(TutorialSave は段階の添字・ページ・案内の番号を持つ。10 §9.4)"
        );
    }

    #[test]
    fn track_money_ends_9_and_quit_ends_15() {
        assert_eq!(
            stage_pages(TutorialStageId(9)).last(),
            Some(&txt::TUT_S09_P4)
        );
        assert_eq!(
            stage_pages(TutorialStageId(15)).last(),
            Some(&txt::TUT_S15_P3)
        );
    }

    #[test]
    #[should_panic(expected = "存在しない")]
    fn unknown_stage_panics() {
        let _ = stage_title(TutorialStageId(13));
    }

    #[test]
    fn only_2_to_6_restrict_and_only_7_9_to_12_supersede() {
        for s in &MAIN_STAGES {
            let early = (2..=6).contains(&s.id.0);
            let all = s.permit_wait == PermitRule::All && s.permit_guide == PermitRule::All;
            assert_eq!(!early && s.id.0 != 1, all, "#{}", s.id.0);
            let sup = matches!(s.id.0, 7 | 9..=12);
            assert_eq!(s.supersedable, sup, "#{}", s.id.0);
        }
    }
}
