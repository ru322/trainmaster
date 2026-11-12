//! `SdlPlatform<'tc>`(`Platform` の実装)と、初期化と終了の順序 [14 §3.2、§3.8]。

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use sdl2::controller::GameController;
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::render::TextureCreator;
use sdl2::video::{FullscreenType, WindowContext};
use sdl2::{GameControllerSubsystem, JoystickSubsystem};
use tm_platform_api::{
    AssetBundle, AudioCmd, DrawList, LOGICAL_HEIGHT, LOGICAL_WIDTH, MonoTime, PhysicalInput,
    Platform, PlatformError, PlatformStats, RENDER_HEIGHT, RENDER_WIDTH,
};

use crate::audio::device::AudioOut;
use crate::clock::{Clock, FrameTimer, PresentTimes};
use crate::config::SdlConfig;
use crate::core::SdlCore;
use crate::diag::DrawLog;
use crate::input::{InputState, PadBackend};
use crate::render::Renderer;

const MAX_FAILED_FRAMES: u32 = 60;

/// 開いているゲームパッド(インスタンス ID → `GameController`)。
struct Pads {
    subsystems: Option<(GameControllerSubsystem, JoystickSubsystem)>,
    open: HashMap<u32, GameController>,
    unmapped_logged: BTreeSet<String>,
}

impl Pads {
    /// マッピングのないジョイスティックなら true。名前と GUID は GUID ごとに 1 回だけログに出す(14 §3.6.2)。
    fn log_unmapped(&mut self, device_index: u32) -> bool {
        let Some((gc, joy)) = self.subsystems.as_ref() else {
            return false;
        };
        if gc.is_game_controller(device_index) {
            return false;
        }
        let name = joy.name_for_index(device_index).unwrap_or_default();
        let guid = joy
            .device_guid(device_index)
            .map(|g| g.string())
            .unwrap_or_default();
        if self.unmapped_logged.insert(guid.clone()) {
            log::warn!("マッピングのないジョイスティックは開かない: {name} GUID {guid}");
        }
        true
    }
}

impl PadBackend for Pads {
    fn open(&mut self, device_index: u32) -> Option<u32> {
        if self.log_unmapped(device_index) {
            return None;
        }
        let (gc, _) = self.subsystems.as_ref()?;
        match gc.open(device_index) {
            Ok(pad) => {
                let id = pad.instance_id();
                log::info!("ゲームパッド {id}: {}", pad.name());
                self.open.entry(id).or_insert(pad);
                Some(id)
            }
            Err(e) => {
                log::warn!("ゲームパッド {device_index} を開けない: {e}");
                None
            }
        }
    }

    fn close(&mut self, instance_id: u32) {
        self.open.remove(&instance_id);
    }

    fn joystick_added(&mut self, device_index: u32) {
        self.log_unmapped(device_index);
    }
}

/// `Platform` の SDL2 実装。`TextureCreator` を借用し、テクスチャ `Texture<'tc>` を持つ(14 §3.9)。
pub struct SdlPlatform<'tc> {
    audio: Option<AudioOut>,
    renderer: Renderer<'tc>,
    pads: Pads,
    core: SdlCore,
    tc: &'tc TextureCreator<WindowContext>,
    input: InputState,
    pending: Vec<PhysicalInput>,
    clock: Clock,
    timer: FrameTimer,
    stats: PlatformStats,
    failed_frames: u32,
    render_error_logged: bool,
    frame_no: u64,
    bundle: Option<AssetBundle>,
    dev_hotkeys: bool,
    /// F11 でフルスクリーンを切り替えられる(KMSDRM でない、ウィンドウのある環境。KMSDRM では開発用のキーが有効でも切り替えない)。
    fullscreen_key: bool,
    /// F11 でフルスクリーンから戻るウィンドウの大きさ。
    window_size: (u32, u32),
    draw_log: DrawLog,
}

impl<'tc> SdlPlatform<'tc> {
    /// 14 §3.2 の手順 5、7、8。`core` を受け取って所有し、`tc` を借用する。
    pub fn new(
        mut core: SdlCore,
        tc: &'tc TextureCreator<WindowContext>,
        cfg: &SdlConfig,
    ) -> Result<SdlPlatform<'tc>, PlatformError> {
        let renderer = Renderer::new(&mut core.canvas, cfg.batch);
        let fullscreen_key = !core.kmsdrm;
        let audio = if cfg.audio.enabled {
            match core
                .sdl
                .audio()
                .and_then(|a| AudioOut::open(&a, &cfg.audio))
            {
                Ok(a) => Some(a),
                Err(e) => {
                    log::warn!("音のデバイスを開けないので音なしで続ける: {e}");
                    None
                }
            }
        } else {
            None
        };
        let mut pads = Pads {
            subsystems: core.pads.clone(),
            open: HashMap::new(),
            unmapped_logged: BTreeSet::new(),
        };
        let mut input = InputState::default();
        let mut pending = Vec::new();
        let count = pads
            .subsystems
            .as_ref()
            .and_then(|(_, joy)| joy.num_joysticks().ok())
            .unwrap_or(0);
        for i in 0..count {
            input.pad_added(i, &mut pads, &mut pending);
        }
        Ok(SdlPlatform {
            audio,
            renderer,
            pads,
            core,
            tc,
            input,
            pending,
            clock: Clock::new(),
            timer: FrameTimer::new(cfg.vsync, cfg.frame_cap),
            stats: PlatformStats::default(),
            failed_frames: 0,
            render_error_logged: false,
            frame_no: 0,
            bundle: None,
            dev_hotkeys: cfg.dev_hotkeys,
            fullscreen_key,
            window_size: (
                LOGICAL_WIDTH * u32::from(cfg.window_scale.max(1)),
                LOGICAL_HEIGHT * u32::from(cfg.window_scale.max(1)),
            ),
            draw_log: DrawLog::default(),
        })
    }

    /// いまのフレームの画素(RGBA、描画の解像度 1920×1080、行優先)。ウィンドウに直接描いたときはウィンドウから読む。
    pub fn capture_frame(&mut self) -> Result<Vec<u8>, PlatformError> {
        self.renderer
            .read_frame(&mut self.core.canvas)
            .map_err(PlatformError::Render)
    }

    /// 直前の `present` の描画の時間(µs)。始めから SDL の present の直前までで、sleep と vsync の待ちを含まない。
    pub fn render_us(&self) -> u32 {
        self.timer.render_us
    }

    /// 音のコールバックが呼ばれた回数。音なしなら `None`。
    pub fn audio_callback_count(&mut self) -> Option<u64> {
        self.audio.as_mut().map(AudioOut::callback_count)
    }

    pub(crate) fn handle_event(&mut self, event: Event, out: &mut Vec<PhysicalInput>) {
        match event {
            Event::KeyDown {
                scancode: Some(Scancode::F11),
                repeat: false,
                ..
            } if self.fullscreen_key => self.dev_hotkey(Scancode::F11),
            Event::KeyDown {
                scancode: Some(Scancode::F12),
                repeat: false,
                ..
            } if self.dev_hotkeys => self.dev_hotkey(Scancode::F12),
            Event::RenderTargetsReset { .. } => self.renderer.invalidate_layers(),
            Event::RenderDeviceReset { .. } => {
                log::warn!("レンダラのデバイスがリセットされたのでテクスチャを作り直す");
                if let Err(e) =
                    self.renderer
                        .reset_device(&mut self.core.canvas, self.tc, self.bundle.as_ref())
                {
                    log::error!("テクスチャを作り直せない: {e}");
                }
            }
            // SDL 2.24 以降は開いていないデバイスの抜けを which = 0 で送る。開く再生デバイスは 1 つなので 0 以外は自分のもの
            Event::AudioDeviceRemoved {
                which,
                iscapture: false,
                ..
            } if which != 0 => {
                if self.audio.take().is_some() {
                    log::warn!("音のデバイス {which} が外されたので音なしで続ける");
                }
                self.stats.audio.enabled = false;
            }
            other => self.input.handle(&other, &mut self.pads, out),
        }
    }

    fn dev_hotkey(&mut self, sc: Scancode) {
        match sc {
            Scancode::F11 => {
                let (w, h) = self.window_size;
                let window = self.core.canvas.window_mut();
                let next = match window.fullscreen_state() {
                    FullscreenType::Off => FullscreenType::Desktop,
                    _ => FullscreenType::Off,
                };
                if let Err(e) = window.set_fullscreen(next) {
                    log::warn!("F11: {e}");
                }
                if next == FullscreenType::Off {
                    if let Err(e) = window.set_size(w, h) {
                        log::warn!("F11: {e}");
                    }
                    window.set_position(
                        sdl2::video::WindowPos::Centered,
                        sdl2::video::WindowPos::Centered,
                    );
                }
            }
            Scancode::F12 => {
                let path = format!("/tmp/trainmaster-{}.rgba", self.frame_no);
                match self.capture_frame() {
                    Ok(px) => match std::fs::write(&path, px) {
                        Ok(()) => log::info!("F12: {path}({RENDER_WIDTH}×{RENDER_HEIGHT} RGBA)"),
                        Err(e) => log::warn!("F12: {path}: {e}"),
                    },
                    Err(e) => log::warn!("F12: {e:?}"),
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
impl<'tc> SdlPlatform<'tc> {
    pub(crate) fn renderer_mut(&mut self) -> &mut Renderer<'tc> {
        &mut self.renderer
    }

    pub(crate) fn canvas_mut(&mut self) -> &mut sdl2::render::Canvas<sdl2::video::Window> {
        &mut self.core.canvas
    }

    /// `present` の描画だけを行う(`debug_assert!`、待ち、計測なし)。
    pub(crate) fn draw_only(&mut self, list: &DrawList) -> crate::render::DrawResult {
        self.renderer.draw(&mut self.core.canvas, self.tc, list)
    }

    /// KMSDRM(実機)と同じく F11 を受け付けない状態にする。
    pub(crate) fn act_as_kmsdrm(&mut self) {
        self.fullscreen_key = false;
    }

    /// レンダーターゲットが使えない場合(層をその場で描く)にする。
    pub(crate) fn force_direct(&mut self) {
        self.renderer.inject.frame_tex = true;
        let r = self
            .renderer
            .reset_device(&mut self.core.canvas, self.tc, self.bundle.as_ref());
        assert_eq!(r, Ok(()));
        assert!(self.renderer.is_direct());
    }
}

impl Platform for SdlPlatform<'_> {
    fn now(&self) -> MonoTime {
        MonoTime(self.clock.now_us())
    }

    fn poll_input(&mut self, out: &mut Vec<PhysicalInput>) {
        out.append(&mut self.pending);
        while let Some(event) = self.core.event_pump.poll_event() {
            self.handle_event(event, out);
        }
    }

    fn load_assets(&mut self, assets: &AssetBundle) -> Result<(), PlatformError> {
        if self.bundle.is_some() {
            return Err(PlatformError::Assets("load_assets は 1 回だけ".to_owned()));
        }
        assets
            .validate()
            .map_err(|e| PlatformError::Assets(format!("{e:?}")))?;
        self.bundle = Some(*assets);
        self.renderer
            .load(self.tc, assets)
            .map_err(PlatformError::Assets)?;
        let t = Instant::now();
        assets.prefault();
        log::info!("音のページの読み込み: {}ms", t.elapsed().as_millis());
        if let Some(audio) = &mut self.audio {
            audio.start(assets.sounds);
            self.stats.audio.enabled = true;
        }
        Ok(())
    }

    fn present(&mut self, frame: &DrawList) -> Result<(), PlatformError> {
        let start = self.clock.now_us();
        let r = self.renderer.draw(&mut self.core.canvas, self.tc, frame);
        let recreates = self.renderer.layer_recreates();
        for w in self
            .draw_log
            .check(&r, recreates, start, cfg!(debug_assertions))
        {
            log::warn!("{w}");
        }
        debug_assert!(
            !r.broken_layer,
            "入れ子か End のない BeginCachedLayer(14 §3.5.5)"
        );
        match r.error {
            Some(e) => {
                self.failed_frames += 1;
                if !self.render_error_logged {
                    self.render_error_logged = true;
                    log::error!("描画のエラー: {e}");
                }
                if self.failed_frames >= MAX_FAILED_FRAMES {
                    return Err(PlatformError::Render(e));
                }
            }
            None => self.failed_frames = 0,
        }
        let drawn = self.clock.now_us();
        let wait = self.timer.wait_before_present(drawn);
        if wait > 0 {
            std::thread::sleep(Duration::from_micros(wait));
        }
        self.core.canvas.present();
        let end = self.clock.now_us();
        let times = PresentTimes { start, drawn, end };
        if self.timer.record(times) {
            log::warn!("vsync が効いていないので、ソフトウェアのフレーム制限に切り替えた");
        }
        self.stats.frame_interval_us = self.timer.interval_us;
        self.stats.present_us = self.timer.present_us;
        self.stats.draw_quads = r.quads;
        self.stats.layer_redraws = self.renderer.layer_redraws;
        self.frame_no += 1;
        Ok(())
    }

    fn audio(&mut self, cmds: &[AudioCmd]) {
        let now = self.clock.now_us();
        if let Some(audio) = &mut self.audio {
            audio.send(cmds, now, &mut self.stats.audio);
        }
    }

    fn logical_size(&self) -> (u32, u32) {
        (LOGICAL_WIDTH, LOGICAL_HEIGHT)
    }

    fn stats(&self) -> PlatformStats {
        self.stats
    }
}
