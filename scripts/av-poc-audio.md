# av_poc による音の実機確認(PL-V-09・PL-V-16)

14 §7.3 の「形式とバッファ、途切れ」と「遅れ」の手順。`av_poc` は 03 §9 の手順 2 の方法で開発用イメージ(`stress-ng` 入り)に入れる。`av_poc` を動かす前に、ゲームを止める(`touch /run/trainmaster.hold; killall trainmaster 2>/dev/null; true`。戻すのは `rm /run/trainmaster.hold`。`scripts/av-poc-device.md` §2)。結果は、IMP-006 の実機確認では `scripts/av-poc-device.md` の記録の表 R6 に、IMP-503 で測り直すときは IMP-503 の記録に書く。

## PL-V-09 オーディオスレッドの優先度、途切れ、CPU 時間

1. `SDL_VIDEODRIVER=kmsdrm SDL_AUDIODRIVER=alsa TM_AUDIO_BUFFER=1024 ./av_poc 2> av-1024.log` で起動する。BGM、効果音(100ms ごと)、ジングル(10 秒ごと)、BGM のクロスフェード(15 秒目から 20 秒ごと)が鳴る。
2. ログの最初の `av_poc: pid <pid>、…` の pid と、`音のデバイス(alsa): …Hz、…ch、…、… サンプル` の行を記録する(PL-V-01)。
3. 優先度: 別の端末で `ps -L -o tid,cls,rtprio,pri,ni,comm -p <pid>` を実行し、SDL のオーディオのスレッド(`comm` が `SDLAudio…`)の `cls`(TS・FF・RR)と `rtprio` を記録する。
4. 途切れ: 5 分そのまま動かし、続く 5 分は別の端末で `stress-ng --cpu 3 --timeout 300s` を動かす。合計 10 分で Esc を押す。その間、耳で途切れ・プツという音・音の高さの狂いがないかを聞く。
5. CPU 時間: 負荷なしと負荷ありの各 1 分、`top -H -d 1 -p <pid>` で、オーディオのスレッドの `%CPU` の最大を記録する(ミキサーの CPU 時間)。
6. ロックの時間: ログの 1 秒ごとの行の `audio() 最大 …us`(`Platform::audio` がロックを取って命令を渡す時間の最大)の最大を記録する(`grep -o 'audio() 最大 [0-9]*' av-1024.log | sort -k3 -n | tail -1`)。
7. ログの最後の 1 秒ごとの行の `音 AudioStats { … }` から `late_callbacks`(累計)、`dropped_cmds`、`dropped_jingles`、`invalid_cmds` を、各行から `max_callback_gap_us` の最大を記録する(`grep -o 'max_callback_gap_us: [0-9]*' av-1024.log | sort -k2 -n | tail -1`)。
8. `TM_AUDIO_BUFFER=2048` で 1〜7 を繰り返す。

- 合格: 1024 で、負荷の下でも途切れが聞こえず、`late_callbacks` が 10 分で 0。`dropped_cmds`・`dropped_jingles`・`invalid_cmds` が 0。オーディオのスレッドの `%CPU` が 1% 未満(14 §4.7)。`audio() 最大` が 1ms 未満。
- 不合格:
  - 優先度が TS のままで途切れる: SDL2 が優先度を上げられていない。03 と起動の権限を見直す。
  - 2048 で合格する: 既定を 2048 にする案を出す(遅れは下で測り直す)。
  - `audio() 最大` が大きく、それと同じ時刻に途切れる: PL-D-02(ロックの受け渡し)をリングバッファにする案を出す。

## PL-V-16 遅れ

`AV_POC_MODE=latency` を足して起動する。1 秒ごとと、Space・Enter・パッドのボタンを押したフレームで、画面全体を 1 フレーム白くし、同じフレームで `se_cursor` を鳴らす。バッファ 1024 と 2048 の両方で行う。

### 操作から音まで(PL-V-16 の合否)

1. マイクをキーボード(またはパッドのボタン)とスピーカーの両方の音が入る位置に置き、録音する。
2. Space を 10 回、1 秒以上の間をあけて押す(1 秒ごとの自動の音と重ならないように、自動の音の間に押す)。
3. 波形で、キーの押下の音の立ち上がりから、次の `se_cursor` の立ち上がりまでの時間を 10 回分測る。キーの押下からスキャンまでの遅れも含む。

- 合格: 10 回の最大が 150ms 以内。
- 不合格: バッファを 512 にして、PL-V-09 の途切れとの兼ね合いを見る。

### 画面と音のずれ(参考)

1. 画面とスピーカーを、音つきの動画(60fps 以上)で撮る。
2. 白いフラッシュが映ったフレームの時刻と、`se_cursor` の立ち上がりの時刻の差を 10 回分測る。この差は「音の遅れ − 画面の遅れ」で、画面の遅れは差し引かれる。
