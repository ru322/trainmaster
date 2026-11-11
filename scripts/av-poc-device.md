# 実機で描画・入力・音を確かめる(IMP-006)

ラズパイ3B の実機で、開発用イメージに入っている `av_poc` を動かし、画面・キーボード・ゲームパッド・音がゲームに使えるかを確かめる手順。16 §2.10 の IMP-006 の行の検証をすべてここで行う。

- 各節に「やること」「見るところ」「合格の目安」を書いた。結果は、いちばん下の「記録の表」に書く。
- 合格しないものがあっても、最後まで進めて記録する(どこまで動くかが分かることが大事)。
- ログの文字(`[INFO] …`)は、そのまま写す。数字は「だいたい」でなく、ログの値を書く。

## 0. 用意するもの

| もの | 使う節 | 注意 |
|---|---|---|
| Raspberry Pi 3 Model B、5V 2.5A 以上の電源 | すべて | 電源が弱いと速さや音の結果が狂う |
| microSD カード(1GB 以上) | すべて | |
| FullHD(1920×1080)の HDMI ディスプレイ。**スピーカー付き**(HDMI で音が出るもの) | すべて | 展示で使うものがあればそれを使う |
| 音の出ないディスプレイ(DVI 変換をはさむもの)。なければ省いてよい | §6.1 | 省いたら記録に「なし」と書く |
| 3.5mm ジャックにつなぐスピーカーかイヤホン | §6 | 耳に当てる前に音量を下げておく |
| USB キーボード | §5 | |
| USB の有線ゲームパッド(XInput 互換。Xbox 360 用など) | §5 | 展示で使う機種がまだ決まっていなければ、手元のもの |
| USB シリアル変換アダプタ(3.3V) | §2〜 | つなぎ方は `br2-external/board/rpi3-64/readme.txt` の 3 |
| LAN ケーブルと、DHCP のあるネットワーク | §2〜 | SSH で 2 つ目の端末を使う |
| スマートフォンなど、録音と動画が撮れるもの | §6.3 | |
| 開発機の、音の波形を見るソフト(Audacity など) | §6.3 | 録音を開発機に移して、波形で時間を測る |

## 1. イメージを作って SD カードに書く

開発機で、リポジトリのいちばん上のディレクトリで:

```sh
mise run br:build-dev
```

できるもの: `build/rpi3-64-dev/images/sdcard.img`(開発用)。SD カードへの書き方は `br2-external/board/rpi3-64/readme.txt` の 2 のとおり。**配布用(`build/rpi3-64/`)と取り違えない**(配布用には `av_poc` が入っていない)。

## 2. 起動して、端末を 2 つ用意する

1. HDMI ディスプレイ、キーボード、LAN ケーブル、シリアルをつないでから電源を入れる。
2. シリアルの端末(開発機の `picocom -b 115200 /dev/ttyUSB0`)に `trainmaster login:` が出たら `root` でログインする。
3. 起動のログの中に `Starting audio: hdmi (1.04s)` のような行がある。この行を写す(§6.1 で使う)。
4. SSH を使えるようにする:
   ```sh
   passwd                 # root のパスワードを決める(SSH は空のパスワードを受け付けない)
   ip -4 addr show eth0   # inet の後ろの数字(例 192.168.1.23)がラズパイのアドレス
   ```
5. 開発機から `ssh root@192.168.1.23`(数字は自分の値)で入る。これで端末が 2 つになる。
   - **端末 A**: `av_poc` を動かす端末(シリアルでも SSH でもよい)
   - **端末 B**: ログを見る、負荷をかける、`kill` する端末(SSH)

`av_poc` を動かす前に、端末 A で毎回これを打つ(ログインし直したら、再起動したら、もう一度):

```sh
touch /run/trainmaster.hold; killall trainmaster 2>/dev/null; true   # ゲームを止める(下の説明)
export SDL_VIDEODRIVER=kmsdrm SDL_AUDIODRIVER=alsa
mx() { grep -o "$1 [0-9]*" "$2" | awk '{ print $NF }' | sort -n | tail -n 1; }   # いちばん大きい値
sm() { grep -o "$1 [0-9]*" "$2" | awk '{ s += $NF } END { print s + 0 }'; }      # 合計
```

端末 B にも `mx` と `sm` の 2 行を打っておく。

**ゲームを止める・戻す**: 新しい開発用イメージでは、電源を入れると HDMI の画面(tty1)でゲーム(`trainmaster`)が自動で始まる。ゲームが画面と音を使っていると `av_poc` が動かないので、上の 1 行目で止めている。

- `/run/trainmaster.hold` があるあいだ、ゲームは起動し直さない。`/run` は再起動で空になるので、**再起動したら 1 行目をもう一度打つ**。
- 止まったかは `pidof trainmaster` で見る(何も出なければ止まっている)。
- 全部の確認が終わったら `rm /run/trainmaster.hold` で戻す(1〜2 秒でゲームが始まる)。
- 古い開発用イメージ(ゲームが自動で始まらないもの)で打っても害はない(止めるものがないだけ)。

**ログの見方**: `av_poc` は 1 秒に 1 回、次のような行を出す。

```
[INFO] 60 fps、間隔 16667us、present 16650us(描画 3500us)、1 秒の最大 間隔 16900us・描画 4200us、遅れたフレーム 0、四角形 2396、層の描き直し 3、音 AudioStats { … late_callbacks: 0, max_callback_gap_us: 47000 }、コールバック Some(860)、audio() 最大 12us
```

| 文字 | 意味 |
|---|---|
| `60 fps` | この 1 秒に描いた回数 |
| `間隔` | 画面を出してから次に出すまでの時間(60Hz なら約 16667us。1 秒 = 1,000,000us) |
| `描画` | 描く命令を出すのにかかった CPU の時間(vsync の待ちを含まない)。`・描画` はこの 1 秒のいちばん大きい値 |
| `遅れたフレーム` | 間隔が予定の 1.5 倍を超えた回数(この 1 秒) |
| `層の描き直し` | 地図の層(緑の格子)を描き直した回数の合計。5 秒に 1 回増える |
| `音 AudioStats {…}` | 音の数え上げ。`late_callbacks` は音のデータを渡すのが遅れた回数の合計 |

**ログを開発機に持ってくる**: 開発機で `ssh root@192.168.1.23 cat /tmp/draw-a.log > draw-a.log`。**`/tmp` は再起動すると消える**ので、再起動する前に、それまでのログを全部持ってくる(`ssh root@192.168.1.23 'cd /tmp && tar cf - *.log' > imp006-logs.tar`)。

## 3. 起動したらまず見るところ(BV6、BV7、BV11 の一部)

端末 B で、順に打って結果を記録の表 R3 に写す。

| やること | 見るところ | 合格の目安 |
|---|---|---|
| `dmesg \| grep -i -E 'vc4\|drm'` | `[drm] Initialized vc4` の行があるか。`error` や `failed` の行がないか | Initialized がある |
| `ls -d /sys/module/vc4; grep -c '^vc4 ' /proc/modules` | 1 行目がディレクトリの名前、2 行目が `0` なら vc4 はカーネルに組み込み(モジュールではない) | 組み込み |
| `ls /dev/dri /dev/fb0` | `card0`(や `card1`)、`renderD128`、`/dev/fb0` があるか | `card` と `renderD128` がある。`fb0` はあれば記録 |
| `grep -i cma /proc/meminfo` | `CmaTotal` と `CmaFree`(av_poc を動かしている間にもう一度見る) | 記録だけ |
| `cat /sys/class/drm/card*-HDMI-A-1/status`、`head -n 3 /sys/class/drm/card*-HDMI-A-1/modes` | `connected`。modes の 1 行目がディスプレイのおすすめの解像度 | 1 行目が `1920x1080`(BV7) |
| `cat /proc/asound/cards`、`aplay -l` | カードの名前。`[vc4hdmi ]` と `[Headphones ]` のような名前があるか | 2 つとも出る。名前が違えば、そのまま写す(`S30audio` を直すのに使う) |
| `grep snd_bcm2835 /proc/modules` | 3.5mm のドライバ(モジュール)が自動で読み込まれたか | 1 行出る |
| `cat /run/asound.conf` | `hdmi:CARD=vc4hdmi` を含むか(スピーカー付きの HDMI ディスプレイのとき) | HDMI になっている |
| `cat /sys/class/thermal/thermal_zone0/temp`、`dmesg \| grep -i voltage` | 温度(1000 で割ると ℃)。電圧が足りない警告がないか | 警告がない |

**modes の 1 行目が `1920x1080` でないとき(BV7)**: 記録してから、§4.2 の「cmdline.txt を書きかえる」の手順で `video=HDMI-A-1:1920x1080@60` を足して起動し直す。`head -n 1 /sys/class/drm/card*-HDMI-A-1/modes` ではなく、§4.1 のログの `表示 0 …` の行が `1920×1080@60Hz`、`3 倍` になるかで確かめ、そのまま §4 以降を行う(固定できたかを R3 に書く。固定できなければ、そのモードのまま進めて記録する)。

## 4. 描画(V3、V4、V6、PL-V-03・04・05・15・18、AS-V-07、UI-V-08、BV6、BV7)

### 4.1 基本の形(A)

端末 A で:

```sh
av_poc 2> /tmp/draw-a.log
```

端末 B で `tail -f /tmp/draw-a.log`(やめるときは Ctrl-C。`av_poc` は止まらない)。60 秒たったら、キーボードの Esc で終わる。

**画面で見ること**(動いている間に):

1. 画面全体に絵が出る。上下左右に黒い帯がない(1920×1080 なら 640×360 のちょうど 3 倍)。
2. 絵がぼやけていない。小さな四角(スプライト)と地図の格子の線のふちが、くっきりした四角い点でできている(拡大が「最近傍」になっている)。
3. スプライトが 2000 個、なめらかに動く。カクッと止まる瞬間がない。
4. 左上の文字(`Train Master av_poc …`)が**黄色っぽい色**。白ではない(頂点の色がテクスチャに掛かっている。PL-V-03)。
5. 緑の地図(格子)がゆっくり左右に動く。5 秒ごとに `layer rev N` の数字が増え、灰色の線の色が少し変わる(PL-V-04、UI-V-08)。
6. 右の白い半透明の四角の向こうが透けて見える。右下の黄色の線(放射状)が出ている。
7. 開発機で同じ `av_poc` を動かし(`mise exec -- cargo run --release -p tm-platform-sdl --example av_poc`)、色が同じに見えるか比べる。スマートフォンで両方の画面を撮っておく。

**ログで見ること**(`/tmp/draw-a.log` のはじめの数行):

| 行 | 見るところ | 対応 |
|---|---|---|
| `SDL 2.32.10: ビデオ …、レンダラ …、表示 …、最大テクスチャ …×…` | ビデオの名前(`KMSDRM` など。そのまま写す)、レンダラが `opengles2`、最大テクスチャが 1024×1024 以上 | PL-V-15、V3、AS-V-07 |
| `表示 0 …: デスクトップ 1920×1080@60Hz、いま …、3 倍(1920×1080、余白 左右 0・上下 0)` | 解像度、何倍か、余白 | PL-V-15、BV7 |
| `音のデバイス(alsa): …` | §6 で使う | PL-V-01 |
| `[WARN]` や `[ERROR]` の行 | すべて写す。特に「フレームテクスチャを作れない」「レンダラのデバイスがリセットされた」「描画のエラー」「vsync が効いていない」「アトラス … が大きすぎる」 | PL-V-04、PL-V-05、AS-V-07 |

終わったら、端末 B で数字をまとめる:

```sh
f=/tmp/draw-a.log
grep -c ' fps、' $f                 # 1 秒ごとの行の数(約 60)
grep ' fps、' $f | tail -n 3        # 最後の 3 行(fps と間隔をそのまま写す)
mx '・描画' $f                      # 描画の最大(us)
mx '最大 間隔' $f                   # 間隔の最大(us)
sm '遅れたフレーム' $f              # 遅れたフレームの合計
```

av_poc を動かしている間に、端末 B で `grep VmRSS /proc/$(pidof av_poc)/status`(メモリの使用量。AS-V-07 の参考)と `grep -i cma /proc/meminfo` も記録する。

- **合格の目安**: 60 fps。間隔がほぼ 16667us(16,000〜17,300)。`・描画` の最大が **8000us 以下**(02 §9.2。8000 を超えた秒があれば、その秒の `層の描き直し` が増えていたかも書く)。遅れたフレームの合計が 0 か、ごく少ない。画面の 1〜7 がすべてそのとおり。
- 黒い画面のまま、またはすぐに終わるときは、§9 へ。

### 4.2 設定を変えて比べる(B〜G)

4.1 と同じように 60 秒ずつ動かして、同じ数字を記録の表 R4 に書く。ログの名前を変える(`draw-b.log` など)。

| 記号 | 端末 A で打つもの | 何を比べるか |
|---|---|---|
| B | `TM_RENDER_BATCH=copy av_poc 2> /tmp/draw-b.log` | 描き方を `Copy`(1 つずつ描く)にしたときの `描画`。A より速いか遅いか(PL-V-03) |
| C | `TM_FPS=30 av_poc 2> /tmp/draw-c.log` | 30 fps で、間隔が約 33333us になるか。スプライトの動きが半分の速さになるのは正常 |
| D | `AV_POC_SPRITES=4000 av_poc 2> /tmp/draw-d.log` | スプライトを 2 倍にしても 60 fps を保てるか(余裕の大きさ) |
| E | `AV_POC_SPRITES=8000 av_poc 2> /tmp/draw-e.log` | 4 倍のとき。fps が下がったら、そのときの `描画` も見る(`描画` が 16000us より小さいのに fps が下がるなら、CPU ではなく GPU か画面に出す処理が重い) |
| F | `TM_RENDER_DRIVER=software av_poc 2> /tmp/draw-f.log` | GPU を使わないソフトウェアの描画で動くか、何 fps か(V4。予備案の材料) |
| G | 下の「720p にする」をしてから、A と E をもう一度(`draw-ga.log`、`draw-ge.log`) | ログが `2 倍` になっていること。1080p と 720p で fps が違えば、その差が 1080p に拡大して出す処理の重さ(PL-V-18) |

- vsync が効いているか(PL-V-05)は A で分かる。間隔がほぼ 16667us にそろい、`vsync が効いていないので、ソフトウェアのフレーム制限に切り替えた` の警告が**出ていなければ**、画面の切り替えが垂直同期を待っている(効いていないときは、はじめの 2 秒でこの警告が出る)。
- `TM_VSYNC=off` は、vsync の代わりにソフトウェアで 60 fps に抑えるだけなので、速さの比べには使わない。

**cmdline.txt を書きかえる**(G の 720p、BV7 の 1920x1080 の固定、50Hz で使う。再起動の前に、ログを開発機に持ってくる):

```sh
mount /dev/mmcblk0p1 /mnt
[ -f /mnt/cmdline.txt.orig ] || cp /mnt/cmdline.txt /mnt/cmdline.txt.orig   # 元のものを 1 回だけ残す
cp /mnt/cmdline.txt.orig /mnt/cmdline.txt
sed -i 's/$/ video=HDMI-A-1:1280x720@60/' /mnt/cmdline.txt   # 720p のとき。ほかは 1920x1080@60 や 1920x1080@50
cat /mnt/cmdline.txt                                          # 1 行のままで、最後に video=… が付いていること
umount /mnt && reboot
# 起動したら cat /proc/cmdline で video= が入っていることを確かめてから行う
# 戻す:
mount /dev/mmcblk0p1 /mnt && cp /mnt/cmdline.txt.orig /mnt/cmdline.txt && umount /mnt && reboot
```

- **起動しなくなったとき**: 電源を切って SD カードを開発機に差す。`bootfs` などの名前で見える FAT のパーティションの `cmdline.txt.orig` を `cmdline.txt` に上書きコピーして、安全に取り出してから差し戻す。

**720p にする**(G のときだけ。終わったら必ず戻す): 上の手順の `1280x720@60` のまま行う。

**60Hz でないとき(できれば)**: 上の手順の `1280x720@60` を `1920x1080@50` にして A を行う。`表示 0 …` の行が `@50Hz` になり、間隔が約 20000us になるか、警告が出ないかを見る(PL-V-05 の後半)。ディスプレイが 50Hz を受け付けなければ「できない」と書く。

## 5. 入力(V4、V10、PL-V-07・08、PL-V-06 (2)(3)、UI-V-04)

### 5.1 キーボード

1. 端末 A で `av_poc 2> /tmp/input-key.log`、端末 B で `tail -f /tmp/input-key.log`。
2. キーボードで、↑ ↓ ← → Enter Space Backspace Tab W A S D Z X C E R を 1 回ずつ押して離す(Q は押さない。Q も Esc と同じく `av_poc` を終わらせる)。
3. 押すたびに `入力: KeyDown(Up)`、離すたびに `入力: KeyUp(Up)` のような行が出るか見る。
4. Esc で終わる。

- **合格の目安**: どのキーも KeyDown と KeyUp が 1 回ずつ出る。押しっぱなしにしても KeyDown が繰り返し出ない(キーリピートは出さない約束)。

### 5.2 ゲームパッド

1. **evtest**: パッドをつなぎ、端末 B で `evtest` を打つ。一覧からパッドの番号を選び、ボタンとスティックを動かして `EV_KEY` と `EV_ABS` の行が出るか見る。Ctrl-C で終わる。一覧の名前を写す(BV8 の材料)。
2. **起動の前からつないであるパッド**(PL-V-07、UI-V-04): パッドをつないだまま、端末 A で `av_poc 2> /tmp/input-pad.log`。ログのはじめに `ゲームパッド N: 名前` と `入力: PadConnected(PadId(N))` が出るか見る。
   - `マッピングのないジョイスティックは開かない: 名前 GUID …` が出たら、その行をまるごと写す(SDL がこのパッドを知らない。マッピングを足す材料)。
3. ボタン(A、B、X、Y、Start、Back、十字キー、LB、RB)を 1 つずつ押して離す。`PadButtonDown` と `PadButtonUp` が出るか。左スティックを上下左右に倒して戻す。`PadAxis` が出るか(半分より多く倒したときと戻したときだけ出る)。
4. **抜き差し**: 動かしたまま、パッドを抜く → `PadDisconnected` が出る。差し直す → `PadConnected` が出る。そのときの `PadId(N)` の数字を書く(差し直すと数字が変わるか。UI-V-04)。
5. **押したまま抜く**: ボタン A を押したまま抜く → `PadButtonUp` が `PadDisconnected` より**先に**出るか。
6. **起動の後でつなぐ**: パッドを抜いて Esc で終わり、パッドなしで `av_poc` を起動してから、パッドをつなぐ → `PadConnected` が出るか。

7. **xpad のドライバ**: パッドをつないだまま、端末 B で:
   ```sh
   ls /sys/bus/usb/drivers/xpad/            # 1-1.2:1.0 のような名前があれば、パッドを xpad が受け持っている
   grep -c '^xpad ' /proc/modules           # 0 なら xpad はカーネルに組み込み
   grep -A4 -i -E 'xbox|pad|joy' /proc/bus/input/devices | grep -E 'N: Name|H: Handlers'
   dmesg | grep -i -E 'xpad|input:' | tail -n 5
   ```
   XInput 互換でないパッドなら、`xpad` の名前はなく、`usbhid` が受け持つ(どちらかを書く)。
8. **udev なしの抜き差し**(V10): 端末 B で `/etc/init.d/S10udevd stop`。そのあと 2〜6 のうち、2(起動の前からのパッド)と 4(抜き差し)をもう一度行う(`input-pad-noudev.log`)。出なければ、`SDL_JOYSTICK_DISABLE_UDEV=1 av_poc 2> /tmp/input-pad-noudev2.log` でもう一度行う(SDL が udev を使わずに `/dev/input` を見張る設定)。終わったら `/etc/init.d/S10udevd start` で戻す(戻さないと、音のカードなどの扱いが変わる)。

- **合格の目安**: 抜き差しのたびに `PadConnected` / `PadDisconnected` が出る。押したまま抜いても `PadButtonUp` が先に出る。ボタンと軸の行が出る(14 §7.3)。udev なしの結果は記録だけ(udev を外せるかの材料)。

### 5.3 終わらせ方と後片付け(PL-V-06 (2)(3)、PL-V-08)

1. **SIGTERM**: 端末 A で `av_poc 2> /tmp/term-1.log`。10 秒たったら端末 B で `kill -TERM $(pidof av_poc)`。端末 A で `echo $?` を打って、終了コードを書く(`0` なら正常)。
2. ログの終わりが、この順になっているか見る:
   ```
   [INFO] 入力: QuitRequested
   [INFO] … フレームで終わる
   [INFO] drop(platform) の前
   [INFO] drop(platform) の後、drop(tc) の前
   [INFO] drop(tc) の後(SDL_Quit まで済んだ)
   ```
3. **SIGINT**: もう一度動かし、端末 A で Ctrl-C を押す。同じ順の行が出るか。
4. **繰り返し**: 端末 A で次を打つ。5 回、起動して 5 秒後に SIGTERM で止める。毎回、画面に絵が出てから消えるかを見る。
   ```sh
   for i in 1 2 3 4 5; do
     (sleep 5; kill -TERM $(pidof av_poc)) &
     av_poc 2> /tmp/term-r$i.log; echo "$i: $?"
   done
   grep -c 'drop(tc) の後' /tmp/term-r*.log   # 5 つとも 1 なら全部の回で後片付けが最後まで済んだ
   ```
5. 止めた後、`av_poc` の絵が画面に残らないか(文字の画面か黒い画面に戻るか)も見る。

- **合格の目安**: SIGTERM でも SIGINT でも、上の順で 5 行が出て、終了コードが 0。5 回とも次の起動で絵が出る(画面と音が解放されている)。

## 6. 音(V12、V14、BV1、BV11、PL-V-01・09・10・16)

音の出力先は、起動のときに `/etc/init.d/S30audio` が決めて `/run/asound.conf` に書く。HDMI を試しに開けたら HDMI、だめなら 3.5mm、それもなければ音なし。手で切り替えるときは:

```sh
/etc/init.d/S30audio start jack   # 3.5mm にする(hdmi・null・auto も使える。auto は起動のときと同じ判定)
cat /run/asound.conf
```

**音が出るかを確かめるとき**は `AV_POC_MODE=latency av_poc 2> /tmp/beep.log` を使う(1 秒ごとに短い音が鳴り、画面が白く光る。Esc で終わる)。

**aplay で鳴らす音**(SDL を通さずに、`S30audio` が決めた既定のデバイスで鳴るかを見る): 端末 B で一度だけ、小さな音(約 440Hz・3 秒・22050Hz・モノラル。最大の音量の 3% ほど)のファイルを作る。

```sh
p=''; i=0; while [ $i -lt 25 ]; do p="$p\350\003"; i=$((i+1)); done
i=0; while [ $i -lt 25 ]; do p="$p\030\374"; i=$((i+1)); done
i=0; while [ $i -lt 1323 ]; do printf "$p"; i=$((i+1)); done > /root/tone.raw
ls -l /root/tone.raw          # 132300 バイトなら正しい
```

鳴らすとき(**先にディスプレイとスピーカーの音量を小さくしておく**。`/dev/urandom` などの雑音は大きな音になるので使わない):

```sh
aplay -t raw -f S16_LE -r 22050 -c 1 /root/tone.raw; echo "rc=$?"
```

### 6.1 出力先と音の形式(V12、BV11、PL-V-01、PL-V-10)

| # | やること | 見るところ |
|---|---|---|
| 1 | スピーカー付きの HDMI ディスプレイで起動する | §2 の `Starting audio: hdmi (…s)` の行。`cat /run/asound.conf` が HDMI |
| 1a | 上の「aplay で鳴らす音」 | ディスプレイのスピーカーから「プー」と 3 秒鳴るか。`rc=0` か。エラーが出たら文を写す(V12・BV11: 既定のデバイス = `plug` → HDMI で、22050Hz・モノラルが鳴る) |
| 2 | `AV_POC_MODE=latency av_poc 2> /tmp/beep-hdmi.log` | ディスプレイのスピーカーから 1 秒ごとに音が鳴るか。ログの `音のデバイス(alsa): 22050Hz、1ch、S16LSB、1024 サンプル` の行(数字をそのまま写す。PL-V-01) |
| 3 | 開発機でも同じものを鳴らす(`AV_POC_MODE=latency mise exec -- cargo run --release -p tm-platform-sdl --example av_poc`) | 音の高さが同じか(違えば、形式の変換がおかしい。PL-V-01) |
| 4 | `av_poc 2> /tmp/music-hdmi.log` を 1 分 | BGM と効果音が鳴るか。「シャリシャリ」「ジー」という雑音がないか(変換の音質。PL-V-10) |
| 5 | `/etc/init.d/S30audio start jack` の後、1a と 2 と 4 をもう一度(3.5mm にスピーカーかイヤホン) | 3.5mm から鳴るか。`amixer -c Headphones sget PCM` の dB の値(0.00dB のはず) |
| 6 | `/etc/init.d/S30audio start null` の後、`av_poc 2> /tmp/null.log` を 10 秒 | 音は出ない。ログに `音のデバイス(alsa)` の行があり、`音のデバイスを開けないので音なしで続ける` が**ない**(音なしでもデバイスを開ける。PL-V-10) |
| 7 | 音の出ないディスプレイ(DVI 変換)で起動し直す | `Starting audio: jack (…s)`。`cat /run/asound.conf` が 3.5mm(`Headphones`)。下の「HDMI の失敗のしかた」を打って、エラーの文と `rc` を写す |
| 8 | ディスプレイを外したまま起動し直す(シリアルで見る) | `Starting audio: jack (…s)`。下の「HDMI の失敗のしかた」を打って、エラーの文と `rc` を写す |
| 9 | 8 のまま、起動の後でスピーカー付きの HDMI ディスプレイをつなぐ | `cat /run/asound.conf` は 3.5mm のまま(起動し直すまで変わらないのが設計どおり)。画面が出るかも見る |
| 10 | `/etc/init.d/S30audio start` で元に戻す | |

**HDMI の失敗のしかた**(7 と 8 で。BV11。`S30audio` はこのエラーで 3.5mm に切りかえている):

```sh
time aplay -t raw -D hdmi:CARD=vc4hdmi,DEV=0 -f S16_LE -c 2 -r 48000 -d 1 /dev/zero; echo "rc=$?"
```

エラーの文(例 `aplay: main:… audio open error: …`)と `rc`、かかった時間(`real`)を写す。スピーカー付きの HDMI ディスプレイで打ったときは、エラーが出ずに約 1 秒で終わる(無音なので何も聞こえない)。
`S30audio` は 3 秒で打ち切るので、`real` が 3 秒を超えるなら、その旨も書く。

- **合格の目安**: 1・7・8 で出力先の判定が上のとおり。判定の時間(`(…s)` の数字)が 1.5 秒くらいまで。HDMI と 3.5mm の両方で、BGM と効果音が雑音なく聞こえる。音の高さが開発機と同じ。

### 6.2 途切れと負荷(V14、BV1、PL-V-09)

手順は `scripts/av-poc-audio.md` の「PL-V-09」の 1〜8 のとおり(バッファ 1024 と 2048、各 10 分。後半の 5 分は `stress-ng --cpu 3 --timeout 300s`)。結果は、`av-poc-audio.md` に書いてある記録先ではなく、このファイルの R6 に書く。読みかえ:

- `./av_poc` は `av_poc` と打つ(イメージでは `/usr/bin/av_poc` に入っている)。先に `cd /tmp` しておく(ログが `/tmp/av-1024.log` などにできる)。
- `ps -L …` と `top -H …` は、開発用イメージに入れた procps-ng のもの(`/bin/ps`、`/bin/top`)を使う。BusyBox の `/usr/bin/top` はスレッドを出せないので、`top` が `-H` を受け付けないときは `/bin/top -H …` と打つ。
- 出力先は HDMI で行う。時間があれば 3.5mm でも 1024 で 1 回行う。

あわせて、同じログから描画の数字も記録する(負荷の前と後で fps と遅れたフレームが変わるか):

```sh
f=/tmp/av-1024.log
grep ' fps、' $f | sed -n '240,300p' | head -n 3   # 負荷なし(4〜5 分目)
grep ' fps、' $f | sed -n '540,600p' | head -n 3   # 負荷あり(9〜10 分目)
sm '遅れたフレーム' $f
cat /sys/class/thermal/thermal_zone0/temp           # 負荷の終わりごろに見る
```

- **合格の目安**(14 §7.3): バッファ 1024 で、負荷の下でも途切れが聞こえず、`late_callbacks` が 10 分で 0。`dropped_cmds`・`dropped_jingles`・`invalid_cmds` が 0。オーディオのスレッドの `%CPU` が 1% 未満。`audio() 最大` が 1000us 未満。

### 6.3 遅れ(PL-V-16)

手順は `scripts/av-poc-audio.md` の「PL-V-16 遅れ」のとおり(バッファ 1024 と 2048)。

- 録音はスマートフォンの録音アプリでよい。録ったファイルを開発機に移し、Audacity などで開く。
- 波形を大きく拡大し、キーの「カチッ」の立ち上がりと、次の音の立ち上がりにそれぞれカーソルを置いて、選んだ範囲の長さ(ms)を読む。これを 10 回分行う。

- **合格の目安**: キーを押してから音が聞こえるまで、10 回の最大が 150ms 以内。

## 7. ビルドの確かめ(開発機。BV2、BV3、BV4、BV6、BV26)

開発機で、`mise run br:build-dev` が終わった後に:

| やること | 見るところ | 合格の目安 |
|---|---|---|
| ビルドのログに `cargo build --offline` が出て、失敗せずに終わったか | `make` が 0 で終わる | BV2(依存の事前取得 `br:cargo-fetch` だけでオフラインのビルドが通る。`.hash` ファイルなし) |
| `build/rpi3-64-dev/host/bin/aarch64-linux-readelf -d build/rpi3-64-dev/target/usr/bin/av_poc \| grep NEEDED` | `libSDL2-2.0.so.0` がある | BV3(`libSDL2` にリンクできた) |
| `mise exec -- make -C build/rpi3-64-dev trainmaster-rebuild V=1 2>&1 \| grep 'cargo build' \| grep -o 'CARGO_PROFILE_RELEASE_[A-Z_]*=[^ ]*'` | `cargo build` の 1 回ごとに同じ名前が 2 回出て、後ろの方が `OPT_LEVEL=3`、`LTO=true`、`CODEGEN_UNITS=1`(前の方は Buildroot の既定の `"2"`・`"false"`・`"16"`。シェルでは後ろが勝つ) | BV4 |
| `grep -E '^CONFIG_(DRM\|SND\|SND_SOC\|DRM_VC4\|JOYSTICK_XPAD)=' build/rpi3-64-dev/build/linux-custom/.config` | 5 つとも `=y` | BV6 |
| `ls -l build/rpi3-64-dev/target/usr/bin/av_poc build/rpi3-64-dev/target/usr/bin/trainmaster` | 2 つともある | BV26 |

## 8. うまくいかないとき

- **`av_poc` がすぐ終わる、画面が黒いまま**: ログの `[ERROR]` の行と、`dmesg | tail -n 30` を写す。`ls /dev/dri` に `card` がなければ vc4 が動いていない(§3 の結果も写す)。
- **`Permission denied` や `Device or resource busy`**: ほかに画面を使っているもの(もう 1 つの `av_poc`)がないか `pidof av_poc` で見る。
- **キーを押すと、ログインの画面に文字が入る**(古い開発用イメージ): HDMI の画面(tty1)のログインの待ち受けにもキーが届いている。`av_poc` の確認には影響しない。気になるときは、文字が入ったログイン画面で Enter を押さずに Ctrl-U で消す。
- **`av_poc` が `Device or resource busy` などで始まらない、ゲームの画面や音が出てくる**: ゲームが動いている。§2 の「ゲームを止める」の 1 行目を打ってから、`pidof trainmaster` で何も出ないことを確かめる。
- **音が鳴らない**: `cat /run/asound.conf`、`aplay -l`、`amixer -c Headphones`(3.5mm のとき)を写す。スピーカーやディスプレイの音量のつまみも確かめる。
- **SSH に入れない**: `passwd` を決めたか、`ip -4 addr` のアドレスが合っているか。

## 9. KMSDRM で表示できなかったとき

§4.1 で絵が出ず、F(ソフトウェアの描画)でも出なければ、02 の D4 の予備案(fbdev に直接書く)に切り替えるかを決める材料になる。次をまとめて報告する:

- §3 の表の結果すべて
- `/tmp/draw-a.log` と `/tmp/draw-f.log` の全体
- `dmesg` の全体(`dmesg > /tmp/dmesg.txt`)
- `ls -l /dev/dri /dev/fb*`、`cat /proc/fb`

---

## 記録の表

数字はログの値をそのまま書く。できなかったものは「できない」と、その理由を書く。

### R0 共通

| 項目 | 結果 |
|---|---|
| 確かめた日、確かめた人 | |
| ディスプレイの機種(スピーカー付き / 音の出ないもの) | |
| ゲームパッドの機種(`evtest` の名前) | |
| キーボードの機種 | |
| 電源(A 数) | |

### R3 起動したらまず見るところ

| 項目 | 結果 | 合否 |
|---|---|---|
| `[drm] Initialized vc4` の行 | | |
| vc4 が組み込みか(`/sys/module/vc4` あり、`/proc/modules` に 0) | | |
| `/dev/dri` の中身、`/dev/fb0` の有無 | | |
| `CmaTotal` / `CmaFree`(起動直後・av_poc の実行中) | | — |
| HDMI の status、modes の 1 行目(BV7) | | |
| 1 行目が 1920x1080 でなかったとき: `video=HDMI-A-1:1920x1080@60` で固定できたか(`表示 0 …` の行) | | |
| `/proc/asound/cards` のカードの名前(すべて写す) | | |
| `snd_bcm2835` が自動で読み込まれたか | | |
| `/run/asound.conf` の中身 | | |
| 温度、電圧の警告 | | |

### R4 描画

| 記号 | fps(最後の 3 行) | 間隔(最後の 3 行) | `・描画` の最大(us) | 間隔の最大(us) | 遅れたフレームの合計 | 画面の様子・警告 | 合否 |
|---|---|---|---|---|---|---|---|
| A 基本 | | | | | | | |
| B copy | | | | | | | |
| C 30fps | | | | | | | |
| D スプライト 4000 | | | | | | | — |
| E スプライト 8000 | | | | | | | — |
| F ソフトウェア | | | | | | | — |
| GA 720p・基本 | | | | | | | — |
| GE 720p・スプライト 8000 | | | | | | | — |
| 50Hz(できれば) | | | | | | | |

| 項目 | 結果 | 合否 |
|---|---|---|
| ビデオの名前、レンダラ、表示(`SDL …` の行をそのまま) | | |
| 最大テクスチャ(AS-V-07。1024 以上か) | | |
| `表示 0 …` の行をそのまま(何倍、余白) | | |
| ウィンドウの大きさが表示モードと同じか(PL-V-15 の後半。**目視で確認**: 絵が画面全体に出て、はみ出しや欠けがない) | | |
| アトラスのテクスチャが作れたか(AS-V-07。`[ERROR]` と `アトラス … が大きすぎる` がない) | | |
| デバイスのリセット・レンダーターゲットの作り直しがあったか(`レンダラのデバイスがリセットされた`・`フレームテクスチャを作れない`・`テクスチャを作り直せない` の行の有無と数。A〜G のログすべてで `grep -c` する) | | |
| 黒い帯がない・ぼやけない・なめらか(§4.1 の 1〜3) | | |
| 文字が黄色(頂点の色。PL-V-03) | | |
| 地図の層が動き、5 秒ごとに描き直される(PL-V-04、UI-V-08) | | |
| 半透明・線(§4.1 の 6) | | |
| 開発機と色が同じ(写真を残した場所) | | |
| `VmRSS`(av_poc の実行中) | | — |
| `[WARN]`・`[ERROR]` の行(すべて写す) | | |

### R5 入力

| 項目 | 結果 | 合否 |
|---|---|---|
| キーボード: KeyDown と KeyUp が出なかったキー | | |
| キーボード: 押しっぱなしで KeyDown が繰り返さない | | |
| `evtest` でボタンと軸が出る | | |
| 起動の前からのパッド: `ゲームパッド N: 名前` と `PadConnected` | | |
| `マッピングのないジョイスティック` の行(出たらそのまま) | | |
| ボタン(出なかったボタン)、軸 | | |
| 抜き差し: `PadDisconnected` / `PadConnected`、PadId の数字の変わり方 | | |
| 押したまま抜く: `PadButtonUp` が先 | | |
| 起動の後でつなぐ: `PadConnected` | | |
| xpad: `/sys/bus/usb/drivers/xpad/` の中身、組み込みか、`/proc/bus/input/devices` の Name と Handlers | | |
| udev なし(`S10udevd stop`): 起動の前からのパッド、抜き差し(既定 / `SDL_JOYSTICK_DISABLE_UDEV=1`) | | — |
| SIGTERM: 5 行の順、終了コード | | |
| SIGINT(Ctrl-C): 5 行の順、終了コード | | |
| 5 回の繰り返し: 終了コード、`drop(tc) の後` の数、毎回絵が出たか | | |
| 止めた後、文字の画面に戻る | | |

### R6 音

| 項目 | 結果 | 合否 |
|---|---|---|
| `aplay -l` のカードの名前 | | |
| aplay の音(既定のデバイス): HDMI で鳴ったか・rc / 3.5mm で鳴ったか・rc | | |
| HDMI の失敗のしかた: 音の出ないディスプレイ(エラーの文、rc、real) | | — |
| HDMI の失敗のしかた: ディスプレイなし(エラーの文、rc、real) | | — |
| `音のデバイス(alsa): …` の行(HDMI / 3.5mm / null) | | |
| 音の高さが開発機と同じ | | |
| HDMI: BGM と効果音、雑音 | | |
| 3.5mm: BGM と効果音、雑音、`PCM` の dB | | |
| null: デバイスが開ける | | |
| 判定: スピーカー付き HDMI → (`Starting audio: …` の行) | | |
| 判定: 音の出ないディスプレイ → | | |
| 判定: ディスプレイなし → | | |
| 起動の後でディスプレイをつないだとき | | |

| 途切れ(PL-V-09) | 1024・HDMI | 2048・HDMI | 1024・3.5mm(できれば) |
|---|---|---|---|
| オーディオのスレッドの `cls` / `rtprio` | | | |
| 途切れ・プツ音・音の高さの狂い(耳) | | | |
| `late_callbacks`(10 分の合計) | | | |
| `max_callback_gap_us` の最大 | | | |
| `dropped_cmds` / `dropped_jingles` / `invalid_cmds` | | | |
| オーディオのスレッドの `%CPU` の最大(負荷なし / あり) | | | |
| `audio() 最大` の最大(us) | | | |
| 負荷なし・ありの fps と間隔、遅れたフレームの合計 | | | |
| 負荷の終わりごろの温度 | | | |
| 合否 | | | |

| 遅れ(PL-V-16) | 1024 | 2048 |
|---|---|---|
| 10 回の値(ms) | | |
| 最大(ms) | | |
| 画面と音のずれ(参考、ms) | | |
| 合否 | | |

### R7 ビルド(開発機)

| 項目 | 結果 | 合否 |
|---|---|---|
| BV2 オフラインのビルド | | |
| BV3 `NEEDED` に `libSDL2` | | |
| BV4 プロファイルの値 | | |
| BV6 カーネルの設定が `=y` | | |
| BV26 `av_poc` と `trainmaster` が入っている | | |

### まとめ(メインエージェントへ)

| 項目 | 結果 |
|---|---|
| KMSDRM + GLES2 で描けたか(だめなら D4 の予備案に切り替えるか) | |
| 既定にするもの: `Geometry` か `Copy`、1080p か 720p、バッファ 1024 か 2048 | |
| `S30audio` で直すところ(カードの名前、判定、時間) | |
| ゲームパッドで足すマッピング | |
| そのほか気づいたこと | |
