# 実機で起動・重さ・メモリ・長時間を確かめる(IMP-503)

ラズパイ3B の実機で、16 §2.10 の IMP-503 の行の検証をする手順。画面・入力・音の基本(IMP-006)は、すでに `scripts/av-poc-device.md` にある。この手順書は、**そこに無いもの**だけを書き、同じことは「av-poc-device.md の §○ を行う」と書いて参照する。

- 各節に「やること」「見るところ」「合格の目安」を書いた。結果は、いちばん下の「記録の表」(H0〜H9)に書く。
- 合格しないものがあっても、最後まで進めて記録する。数字はログの値をそのまま写す。
- 予算(02 §9)を超えた項目は、最後の「H9 まとめ」に集める。これが IMP-504(性能対策)の入力になる。
- 「開発機で」と書いたコマンドは、リポジトリのいちばん上のディレクトリ(`mise.toml` がある所)で打つ。「実機で」と書いたコマンドは、ラズパイのシェル(シリアルか SSH)で打つ。

この手順書で確かめる項目:

| 節 | 項目 | もとの文書 |
|---|---|---|
| §3 | 起動の時間と大きさ、コマンドラインと blacklist の効果、候補(`disable-bt`・`disable-wifi`・`boot_delay=0`)の比較、`getrandom` の待ち | 03 BV9・BV5、02 V15・V13、14 PL-V-11、15 AS-V-10 |
| §4 | シリアル(`ttyAMA0`)、読み取り専用の rootfs のエラー、vc4 の組み込み | 03 BV29・BV12・BV6 |
| §5 | `tm-launch` の待ち・起動し直し・hold のファイル・`tee`・`XDG_CACHE_HOME` | 03 BV30 |
| §6 | ゲームの 1 フレームの重さ(60fps の予算)、描き方の比べ | 02 §9.1・§9.2、V6、13 UI-V-02、14 PL-V-03(本番の絵)、16 APP-V-02(実機) |
| §7 | 経路の計算と 1 tick のベンチマーク(aarch64 でビルドして実機で動かす) | 12 §8.7・RT-V-01、11 VS-05、02 V6 |
| §8 | メモリ(128MB の予算)と 8 時間の放置 | 02 §9.3・V16、14 PL-V-12、15 AS-V-10 |
| §9 | パッド、会場の表示機器の音、音の途切れと遅れの測り直し、ジングルの終わり | 03 BV8・BV11(最終)、13 UI-V-05・UI-V-09、14 PL-V-09・PL-V-10(最終)・PL-V-16 |
| §10 | 画面の読みやすさ(大人が見る分。子どもは `scripts/playtest.md`) | 13 UI-V-01・UI-V-10、15 AS-V-04・AS-V-11・AS-V-13、02 V7 |

## 0. 用意するもの

`scripts/av-poc-device.md` の §0 の表のもの全部に加えて:

| もの | 使う節 | 注意 |
|---|---|---|
| microSD カードを 2 枚(開発用と配布用) | すべて | 1 枚で書き直してもよい。どちらを書いたか、カードにシールを貼っておく |
| 展示で使うゲームパッド(V1 で決まったもの) | §9 | まだ決まっていなければ、手元のもので行い、記録に「仮」と書く |
| 会場と同じ表示機器(ディスプレイ・テレビ) | §9、§10 | なければ手元の FullHD のもので行い、記録にその旨を書く |
| ストップウォッチか、動画が撮れるスマートフォン | §3 | 電源を入れてからの秒数を測る。動画なら後で何度も見直せる |
| 8 時間、電源を切らずに置ける場所 | §8 | 夜のあいだに回してもよい |

## 1. イメージを作って SD カードに書く

開発機で:

```sh
mise run br:build-dev   # 開発用: build/rpi3-64-dev/images/sdcard.img
mise run br:build       # 配布用: build/rpi3-64/images/sdcard.img
```

書き方は `br2-external/board/rpi3-64/readme.txt` の 2 のとおり。どの節でどちらを使うか:

| 節 | 使うイメージ | 理由 |
|---|---|---|
| §3.1〜§3.4、§3.6、§4.2、§4.3 | **配布用** | 展示で使うのは配布用。起動の時間と、読み取り専用の rootfs のエラーは配布用でしか分からない |
| §3.5、§3.7、§4.1、§5〜§9 | **開発用** | シェルに入ってコマンドを打つため。ゲームの実行ファイルは配布用と同じ作り方(release)でできている |
| §10 | どちらでもよい(配布用がよい) | |

## 2. 共通の準備

### 2.1 シリアルの記録を残す

開発機で、シリアルの表示をファイルにも残す(配布用ではシリアルしか見る所がない):

```sh
picocom -b 115200 --logfile serial-boot1.log /dev/ttyUSB0   # 終わるときは Ctrl-A を押してから Ctrl-X
```

つなぎ方は `readme.txt` の 3。起動ごとにファイルの名前を変える(`serial-boot2.log` など)。

### 2.2 開発用イメージで、端末を 2 つ用意する

`scripts/av-poc-device.md` の §2 の 1〜5 のとおり(`passwd`、SSH で端末 B)。ただし **§5〜§9 では、ふだんはゲームを止めない**(ゲームそのものを測る)。ゲームを止める(hold のファイルを置く)のは、§7 のベンチマークと、`av_poc` を使う §9.1 の 1.・§9.2・§9.3 のときだけ。終わったら `rm /run/trainmaster.hold` で戻す。

### 2.3 ゲームに設定を渡す(開発用イメージ)

ゲームの設定は環境変数で、`/etc/default/trainmaster` に書く(16 §1.12)。書いたら `killall trainmaster` で起動し直すと効く(`tm-launch` が 1〜2 秒で起動し直す)。

```sh
echo 'TM_LOG=debug' >> /etc/default/trainmaster   # 例: 10 秒ごとに「計測 600 フレーム」の行を出す
killall trainmaster
cat /etc/default/trainmaster                       # 書いた行を確かめる
```

- 元に戻すときは、足した行を消す(`vi /etc/default/trainmaster`)か、次のように `TM_` で始まる行をすべて消す(元からあるのは `#` の説明の 2 行だけ):
  ```sh
  sed -i '/^TM_/d' /etc/default/trainmaster; killall trainmaster
  ```
- ゲームのログは `/tmp/trainmaster.log`(1 つ前の起動は `/tmp/trainmaster.log.1`)。**`/tmp` は再起動で消える**ので、再起動の前に開発機へ持ってくる: `ssh root@192.168.1.23 cat /tmp/trainmaster.log > game-xxx.log`(数字は自分のアドレス)。

### 2.4 ログの行の読み方

ゲームのログの行は `[    12.345s INFO  tm_app::app] 最初のフレーム: 起動から 3ms` の形。はじめの `12.345s` は、**ゲームのプロセスが始まってからの秒数**。

## 3. 起動の時間と大きさ(BV9、BV5、V15、PL-V-11、AS-V-10、V13)

### 3.1 電源を入れてからタイトル画面まで(配布用、3 回)

1. 配布用の SD を差し、HDMI ディスプレイ・キーボード・シリアルをつなぐ。LAN はつながない。
2. 開発機で §2.1 の `picocom` を始める。
3. スマートフォンで動画を撮り始める(電源のプラグとディスプレイが両方映るように)。
4. 電源を差す。タイトル画面が出たら、さらに 5 秒撮ってから止める。
5. 電源を抜いて 10 秒待ち、2〜4 をあと 2 回くり返す(ログの名前を変える)。

**見るところ**:

| 何を | どこで | 書き方 |
|---|---|---|
| 電源を差してからタイトル画面が出るまでの秒数 | 動画(コマ送りで、プラグが入ったコマと、タイトルの絵が出たコマ) | 0.1 秒の単位で 3 回分 |
| `tm-launch: starting trainmaster at 5s` の行 | シリアルのログ | `at` の後ろの数字(カーネルが動き始めてからの秒数) |
| ゲームの起動の内訳の行(下の表) | シリアルのログ(`tm-launch` がゲームのログもシリアルに出す) | 行をそのまま写す |

シリアルのログから、内訳の行だけを取り出す(開発機で):

```sh
grep -E 'tm-launch:|trainmaster [0-9]|SdlCore::new|SdlPlatform::new|音のデバイス|音のページの読み込み|load_assets|最初のフレーム' serial-boot1.log
```

| 行 | 意味 |
|---|---|
| `SdlCore::new: …ms` | SDL・画面(KMSDRM)・レンダラを開く時間 |
| `音のデバイス(alsa): …` / `SdlPlatform::new: …ms` | 音のデバイスを開く時間を含む |
| `音のページの読み込み: …ms` | 実行ファイルに入った音のデータを読み込む時間(PL-V-11、AS-V-10) |
| `load_assets: …ms` | アトラスの転送と音のページの読み込みの合計 |
| `[ x.xxxs …] 最初のフレーム: …` | **はじめの `x.xxx` 秒が「プロセスの起動から最初の表示まで」**(BV5 の 3 秒の目標と比べる) |

- **合格の目安**: プロセスの起動から最初の表示まで(`最初のフレーム` の行の頭の秒数)が **3 秒以内**(14 PL-D-29)。電源を入れてからタイトル画面まで **20 秒以内**(03 §5.8 の暫定の目標)。
- 3 秒を超えたら、どの行が大きいかを H1 に書く(音のページなら、BGM を短くするか実行時の合成にする。03 BV5 の対策)。

### 3.2 大きさ(配布用)

開発機で、イメージの大きさと、SD に書いた後の使用量を見る。

```sh
ls -l build/rpi3-64/images/sdcard.img build/rpi3-64/images/rootfs.ext4 build/rpi3-64/images/boot.vfat
ls -l build/rpi3-64/target/usr/bin/trainmaster      # ゲームの実行ファイル(音と絵が入っている。約 30MB までの見込み)
```

配布用を書いた SD を開発機に差して(SD カードの番号は `lsblk` で、差す前と後で比べて決める。例 `/dev/sdb`):

```sh
sudo mkdir -p /mnt/tmboot /mnt/tmroot
sudo mount -o ro /dev/sdb1 /mnt/tmboot && sudo mount -o ro /dev/sdb2 /mnt/tmroot
df -h /mnt/tmboot /mnt/tmroot
sudo umount /mnt/tmboot /mnt/tmroot
```

- 自動でマウントされたときは、`df -h` でその場所(`/media/…/bootfs` など)を見ればよい。
- **合格の目安**: boot(64M)に十分な空きがある(03 BD10)。数字は記録だけ(03 §6.2 の開発機の値と比べる)。

### 3.3 コマンドラインの効果(配布用、BV9)

§3.1 の起動を見ながら、それとタイトル画面のまま **15 分**置いて、次を見る:

| 見ること | 合格の目安 |
|---|---|
| HDMI の画面に、カーネルや起動スクリプトの文字が出ないか(`console=tty3`、`quiet`) | 文字が出ない(黒い画面か、ファームウェアの絵だけ) |
| 画面の左上でカーソルが点滅しないか(`vt.global_cursor_default=0`) | 点滅しない |
| 15 分置いて画面が消えないか(`consoleblank=0`。カーネルの既定は 10 分で消える) | 消えない |

### 3.4 rootfs の使用量とコマンドライン(開発用で確かめる分)

`readme.txt` の 5 の「ログインしたら」の 4 行(`cat /proc/uptime`、`cat /proc/cmdline`、`cat /proc/device-tree/model`、`df -h /`、boot の空き)を、**開発用イメージ**で打って写す(配布用にはログインできない)。`cat /proc/cmdline` が `br2-external/board/rpi3-64/cmdline.txt` と同じかを見る。

### 3.5 modprobe の blacklist の効果(開発用、BV9)

実機で:

```sh
for m in bcm2835_v4l2 bcm2835_codec bcm2835_isp bcm2835_mmal_vchiq vc_sm_cma brcmfmac hci_uart; do
  grep -q "^$m " /proc/modules && echo "$m: 読み込まれている" || echo "$m: なし"
done
```

- **合格の目安**: 7 つとも「なし」(blacklist が効いている)。
- 効果の大きさ: blacklist を外して起動の時間を比べる(開発用は書き込める):
  ```sh
  mv /etc/modprobe.d/trainmaster.conf /root/trainmaster.conf.off; reboot
  # 起動したら、シリアルの tm-launch: starting trainmaster at …s を写し、上の for の行をもう一度打つ
  mv /root/trainmaster.conf.off /etc/modprobe.d/trainmaster.conf; reboot   # 必ず戻す
  ```
  ある時とない時の `at …s` を 2 回ずつ書く。

### 3.6 候補の比較(`disable-bt`・`disable-wifi`・`boot_delay=0`。配布用、BV9)

03 §5.8 の候補を `config.txt` に足して、§3.1 と同じ測り方で比べる。`config.txt` は SD の boot(FAT)にあり、開発機で書きかえられる。

1. 配布用の SD を開発機に差し、boot をマウントする(§3.2 と同じ。今度は書き込むので `-o ro` を付けない):
   ```sh
   sudo mount /dev/sdb1 /mnt/tmboot
   [ -f /mnt/tmboot/config.txt.orig ] || sudo cp /mnt/tmboot/config.txt /mnt/tmboot/config.txt.orig   # 元を 1 回だけ残す
   ```
2. 比べる版ごとに、元から作り直してから足す:

| 版 | 開発機で打つもの(1. の後) | 何が変わるか |
|---|---|---|
| A いまのまま | `sudo cp /mnt/tmboot/config.txt.orig /mnt/tmboot/config.txt` | 比べる元 |
| B 待ちなし | A の後に `echo 'boot_delay=0' \| sudo tee -a /mnt/tmboot/config.txt` | ファームウェアがカーネルを読む前の待ちをなくす |
| C BT と無線を消す | A の後に `sudo sed -i 's/^dtoverlay=miniuart-bt$/dtoverlay=disable-bt/' /mnt/tmboot/config.txt` と `echo 'dtoverlay=disable-wifi' \| sudo tee -a /mnt/tmboot/config.txt` | Bluetooth と無線 LAN をデバイスごと消す。`disable-bt` は PL011 を GPIO 14/15 に戻すので、`miniuart-bt` と入れ替える |
| D B と C の両方 | C の後に `echo 'boot_delay=0' \| sudo tee -a /mnt/tmboot/config.txt` | |

3. `cat /mnt/tmboot/config.txt` で中身を確かめ、`sudo umount /mnt/tmboot` してから SD を抜いてラズパイに差す。
4. §3.1 の 1〜5 を、版ごとに 3 回ずつ行う。あわせて、版ごとに次を見る(副作用):
   - シリアルに起動のログと `tm-launch: …` の行が出るか(C・D で `ttyAMA0` が GPIO 14/15 に来ているか)
   - タイトル画面の BGM が鳴るか(HDMI)
   - 画面がふつうに出るか
5. 終わったら A に戻す(`sudo cp /mnt/tmboot/config.txt.orig /mnt/tmboot/config.txt`)。

- **起動しなくなったとき**: SD を開発機に差して、`config.txt.orig` を `config.txt` に上書きする。
- **合格の目安(採るか)**: 起動が速くなり、シリアル・音・HDMI に副作用がなければ「採る」とする(03 §5.8 の推奨)。採るかどうかは H1 の表にメインエージェントへの提案として書く(`config.txt` と blacklist を直すのは IMP-501 の領域)。

### 3.7 `getrandom` の待ち(開発用、V13)

ゲームの種(シード)は、新しいゲームを始めたときに OS の乱数(`getrandom`)から取る。カーネルの乱数の準備ができる前だと待たされる。

1. 開発用で、電源を入れてタイトル画面が出たら、すぐに Enter を押してゲームを始める。ゲームの画面に変わるまでに間があったかを見る(ふつうは一瞬)。
2. 実機で:
   ```sh
   dmesg | grep -i 'crng'                                # 例 [    3.141592] random: crng init done
   grep -E 'tm-launch: starting|新しいゲーム' /tmp/trainmaster.log
   ```
- `crng init done` の時刻(カーネルが動き始めてからの秒数)が、`tm-launch: starting trainmaster at …s` より前なら、ゲームを始める時には準備ができている(待たない)。
- `crng` の行が出ないときは「行なし」と書く。
- **合格の目安**: Enter を押してからゲームの画面までに、目に見える待ちがない。

## 4. シリアル、読み取り専用の rootfs、vc4(BV29、BV12、BV6)

### 4.1 開発用で(BV29)

| やること | 見るところ | 合格の目安 |
|---|---|---|
| 起動してシリアルを見る | 起動のログ、`trainmaster login:` が出るか(開発用の getty) | 出る |
| `ls /dev/ttyAMA*`、`dmesg \| grep tty` | `ttyAMA0` があり、`dmesg` に `ttyAMA0 at MMIO …` と `console [ttyAMA0] enabled` の行があるか | `ttyAMA0` がコンソール(`ttyAMA1` になっていない) |
| `cat /proc/consoles` | `ttyAMA0` と `tty3` が並ぶか | 並ぶ |
| `dmesg \| grep -i -E 'defer\|vc4'` | `-EPROBE_DEFER`(`deferred`)で HDMI が遅れていないか(BV6 の残り) | vc4 が `Initialized` で、遅れの行がないか、あっても起動の時間に響いていない |

BV6 のほかの項目(vc4 が組み込みか、CMA の量、`/dev/fb0`)は `scripts/av-poc-device.md` の §3 で記録済みのものを使う。IMP-006 の後でカーネルの設定を変えたなら、その §3 の表をもう一度行う。

### 4.2 配布用で(BV29、BV12)

§3.1 で残したシリアルのログ(`serial-boot1.log`)を使う。

```sh
grep -n -i -E 'fail|error|read-only|denied|cannot|can.t' serial-boot1.log
```

- **BV29 の合格の目安**: 配布用でもシリアルに起動スクリプトの `Starting …` の行と、`tm-launch: starting trainmaster at …s` とゲームのログが出る(ログインの待ち受けは出ない)。
- **BV12 の合格の目安**: 上の `grep` で出た行がない。出た行はすべて H2 に写す(読み取り専用の rootfs に書こうとするスクリプトを見つける材料。QEMU では `seedrng` のエラーを消した。03 §5.7)。

### 4.3 配布用で書き込みのエラーがないか(BV30 の `XDG_CACHE_HOME` の分)

同じログで:

```sh
grep -n -i -E 'cache|mesa|shader' serial-boot1.log
```

- **合格の目安**: キャッシュを書けないという行(`Read-only file system` など)がない。

## 5. `tm-launch` の動き(開発用、BV30)

`tm-launch` は、ゲームが終わると起動し直す。**起動から 10 秒未満**で終わる回が続くと、起動し直す前に 2、4、8、16 秒待つ(16 秒より長くは待たない)。10 秒以上動いた後に終わったら、すぐ起動し直す。

端末 A(シリアル)で表示を見ながら、端末 B(SSH)で打つ。シリアルには `tm-launch: …` の行が出る。

| # | 端末 B で打つもの | 見るところ(シリアルと HDMI の画面) | 合格の目安 |
|---|---|---|---|
| 1 | ゲームが 10 秒以上動いているときに `killall -ABRT trainmaster`(異常終了のまね) | `tm-launch: trainmaster exited with status 134 after …s` の後、すぐ `tm-launch: starting trainmaster at …s` | 待たずに起動し直す。タイトル画面に戻る |
| 2 | 起動し直したら、5 秒以内にまた `killall -ABRT trainmaster`。これを 5 回続ける(毎回、`starting` の行が出てから 5 秒以内に打つ) | `tm-launch: 1 quick exit(s) in a row, waiting 2s` → `2 …, waiting 4s` → `3 …, waiting 8s` → `4 …, waiting 16s` → `5 …, waiting 16s` | 待ちが 2、4、8、16、16 秒になる |
| 3 | 2 の後、20 秒以上待ってから `killall trainmaster`(ふつうの終わり。SIGTERM) | `exited with status 0`、待たずに `starting` | 待ちが 0 に戻る |
| 4 | `touch /run/trainmaster.hold; killall trainmaster` | `exited …` の後、`starting` が出ない。`pidof trainmaster` が何も出さない | 起動しない |
| 5 | 30 秒待ってから `rm /run/trainmaster.hold` | 1〜2 秒で `starting` が出て、タイトル画面が出る | 起動する |
| 6 | `echo 'TM_FPS=30 oops(' >> /etc/default/trainmaster; killall trainmaster`(書きまちがいのまね) | シリアルに `tm-launch: syntax error in /etc/default/trainmaster, ignored`。HDMI の画面にこの文字が出ないか | シリアルにだけ出る。ゲームは起動する |
| 7 | `sed -i '/oops/d' /etc/default/trainmaster; killall trainmaster` で戻す | | |

1〜7 のあいだ、次も見る(`tee` の影響):

- HDMI の画面(tty1)に、起動し直す間に文字(`tm-launch: …` やログ)が出ないか。
- シリアルの表示が乱れないか(行が混ざって読めなくならないか)。
- `/tmp/trainmaster.log` と、シリアルに出た行が同じか: `cat /tmp/trainmaster.log` とシリアルの最後の起動の分を比べる。

`XDG_CACHE_HOME`(読み取り専用の rootfs へ書かない設定)が効いているか:

```sh
tr '\0' '\n' < /proc/$(pidof trainmaster)/environ | grep -E 'XDG_CACHE_HOME|SDL_'
ls -la /tmp/.cache /root/.cache 2>&1
```

- **合格の目安**: `XDG_CACHE_HOME=/tmp/.cache` が出る。`/root/.cache` がない(あれば、ゲームか Mesa がそこに書いている)。

## 6. ゲームの 1 フレームの重さ(開発用。02 §9.1・§9.2、V6、UI-V-02、PL-V-03、APP-V-02)

ゲームは `TM_LOG=debug` のとき、600 フレーム(60fps で 10 秒)ごとに次の形の行を出す(数字は例):

```
[    20.012s DEBUG tm_app::profiler] 計測 600 フレーム: 平均/最大 µs input 5/33 update 40/210 sim 12/95 render 300/900 out 15000/17300、step 0:0 1:600 2:0 3:0 4+:0、間隔の平均 16667µs、捨てた tick 0、present 15800µs、四角形 820
```

| 文字 | 意味 | 予算(02 §9.2、13 §9) |
|---|---|---|
| `input` / `update` | 入力の処理 / UI の更新(おすすめの問い合わせを含む) | input + update で 1ms(1000µs)以下(UI-V-02 は update 1ms) |
| `sim` | シミュレーション(そのフレームの step の合計) | 平均 2ms(2000µs)以下 |
| `render` | 描く命令(DrawList)を作る時間 | 2ms 以下 |
| `out` / `present` | 音の命令を渡す + 画面に出す時間(**vsync の待ちを含む**ので、ここは予算と比べない) | — |
| `step 0:… 1:… 2:…` | 1 フレームに進めた tick の数ごとのフレーム数。ゲーム中は `1:` がほとんどで、`2:` 以上が少ないほどなめらか | — |
| `間隔の平均` | フレームの間隔。60fps なら約 16667µs | 16667 に近い(30fps の最低条件なら 33333 以下) |
| `捨てた tick` | 処理が遅れて捨てた tick の累計 | 0 |
| `四角形` | そのフレームで描いた四角形の数 | 2000 以下 |

描画そのものの CPU の時間(02 §9.2 の 6〜8ms)は、このログでは分からない(present に vsync の待ちが入る)。それは `av_poc` の `描画` で測ったもの(`scripts/av-poc-device.md` §4、R4)を使う。

### 6.1 測り方

1. 実機で `echo 'TM_LOG=debug' >> /etc/default/trainmaster; killall trainmaster`。
2. 次の 4 つの場面で、それぞれ 1 分以上そのまま置く(ゲームを止める説明が出ている間は、ゲームが止まるので数に入れない。説明は読んで閉じる)。
   - **T** タイトル画面
   - **E** 序盤(路線 1〜2 本、会社 Lv1 くらい)
   - **M** 中盤(会社 Lv4 の後、路線 3〜4 本)
   - **L** 終盤(路線 6 本、線路のマスが多く、路線どうしが同じ区間を共有している所が多い。13 UI-V-02 の「線路のマス 200・6 路線」に近づける。15〜30 分ほど遊ぶとここに来る)
3. 場面ごとに、そのときの時刻をメモしておき、後で開発機へログを持ってきて(§2.3)、その時刻の `計測 600 フレーム` の行を 3 つ写す:
   ```sh
   grep '計測 600 フレーム' game-6-1.log | tail -n 20
   ```
4. あわせて、路線を作った瞬間・のばした瞬間・会社レベルが上がった瞬間に、画面がカクッとしないかを目で見る(経路の再計算。RT-V-04 の試遊の分は `scripts/playtest.md`)。
5. 最大の値をまとめて見る:
   ```sh
   grep -o 'sim [0-9]*/[0-9]*' game-6-1.log | sort -t/ -k2 -n | tail -n 1
   grep -o 'update [0-9]*/[0-9]*' game-6-1.log | sort -t/ -k2 -n | tail -n 1
   grep -o 'render [0-9]*/[0-9]*' game-6-1.log | sort -t/ -k2 -n | tail -n 1
   grep -o '捨てた tick [0-9]*' game-6-1.log | tail -n 1
   grep -E 'WARN|ERROR' game-6-1.log
   ```

### 6.2 描き方と fps を変えて比べる(PL-V-03 の本番の絵、02 §9.1 の 30fps)

L の場面(できなければ M)で、`/etc/default/trainmaster` に 1 行ずつ足して比べる。足したら `killall trainmaster` で起動し直し、また同じ場面まで遊ぶ必要がある。時間がなければ M の場面で行い、その旨を書く。

| 記号 | 足す行(`TM_LOG=debug` に加えて) | 比べること |
|---|---|---|
| G | なし(既定。`render_geometry` でまとめて描く) | 基準 |
| C | `TM_RENDER_BATCH=copy` | `間隔の平均`、`step 2:` 以上の数、見た目が同じか |
| F30 | `TM_FPS=30` | `間隔の平均` が約 33333µs、動きがかくかくしすぎないか |

- **合格の目安**(G で): T・E・M・L のすべてで、`間隔の平均` が 16,000〜17,300µs、`捨てた tick` が 0、`sim` の平均が 2000µs 以下、`update` と `render` の最大がそれぞれ 1000µs・2000µs 以下(UI-V-02)、`四角形` が 2000 以下。
- G より C の方が軽ければ、既定を `Copy` にする案を H3 に書く(14 PL-V-03 の対策)。
- APP-V-02(フレームの間隔のスナップ)の実機の分: ゲームを `killall trainmaster` で終わらせたときにログの最後に出る `step の回数 0/1/2/3/4+: …、間隔のずれ …` の行をそのまま写す。

## 7. 経路の計算と 1 tick のベンチマーク(12 §8.7、RT-V-01、VS-05、V6)

`tm-core` と `tm-pathfind` のベンチマーク(`criterion`)を、開発機で aarch64 向けにビルドし、実機で動かす。

### 7.1 開発機でビルドする

先に `mise run br:build-dev` が終わっている必要がある(Buildroot が作ったクロスのリンカを使う。これでイメージと同じ glibc にリンクされる)。

```sh
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=$PWD/build/rpi3-64-dev/host/bin/aarch64-linux-gcc
mise exec -- cargo bench --locked --target aarch64-unknown-linux-gnu \
  -p tm-core -p tm-pathfind --features tm-core/test-support --bench tick --bench apsp --no-run 2>&1 | grep Executable
```

- 出力の例: `Executable benches/tick.rs (…/target/aarch64-unknown-linux-gnu/release/deps/tick-ea190bf5d77b85b9)`。括弧の中のパスが、実機に送るファイル(`tick-…` と `apsp-…` の 2 つ)。
- `--features tm-core/test-support` を付けると、最悪の場面(48 本が同じ tick に着く、再計算を起こす tick)のベンチマークも入る。
- `BR_OUTPUT` を変えている(`mise.local.toml`)ときは、`build/rpi3-64-dev` をその値の後ろに `-dev` を付けたものに読みかえる。
- 開発機での値も比べるのに使う(3B が何倍遅いか。12 §5.4 は 15〜30 倍と仮定している)。開発機で:
  ```sh
  mise exec -- cargo bench --locked -p tm-core -p tm-pathfind --features tm-core/test-support --bench tick --bench apsp -- --noplot 2>&1 | tee bench-x86.txt
  ```
  開発機で約 1〜2 分かかる。

### 7.2 実機で動かす

1. 開発機から送る(パスは 7.1 で出たもの。アドレスは自分の値):
   ```sh
   scp target/aarch64-unknown-linux-gnu/release/deps/tick-ea190bf5d77b85b9 \
       target/aarch64-unknown-linux-gnu/release/deps/apsp-aa250ec2be6ee4df root@192.168.1.23:/root/
   ```
2. 実機で、ゲームを止めてから(ゲームが CPU を使うと値が狂う)動かす:
   ```sh
   touch /run/trainmaster.hold; killall trainmaster 2>/dev/null; true
   cat /sys/class/thermal/thermal_zone0/temp            # 始める前の温度(1000 で割ると ℃)
   cd /tmp
   CRITERION_HOME=/tmp/criterion /root/tick-ea190bf5d77b85b9 --bench --noplot 2>&1 | tee /tmp/bench-tick.txt
   CRITERION_HOME=/tmp/criterion /root/apsp-aa250ec2be6ee4df --bench --noplot 2>&1 | tee /tmp/bench-apsp.txt
   cat /sys/class/thermal/thermal_zone0/temp            # 終わった後の温度
   rm /run/trainmaster.hold                             # ゲームを戻す
   ```
   - `--bench` を付けないと、測らずに 1 回ずつ試すだけになる。
   - 開発機の何倍もかかる(30 分以上かかることもある)。途中で止めない。
3. 開発機に持ってきて、値の行だけを見る:
   ```sh
   ssh root@192.168.1.23 cat /tmp/bench-tick.txt /tmp/bench-apsp.txt > bench-rpi3.txt
   grep -B1 'time:' bench-rpi3.txt | grep -v '^--'
   ```
   `time: [下 中 上]` の**まん中の値**を記録の表 H4 に写す。

### 7.3 合格の目安

| ベンチマーク | 何か | 合格の目安 |
|---|---|---|
| `routes/recompute_full_tick` | 01 の上限の規模(152 ノード)で、経路の再計算を含む 1 tick | **16ms 以下**(50ms 以下なら許容)。12 §5.6 の SW-1 / SW-2 で判断する |
| `compute/RepeatedDijkstra/152` + `build_next_hops/152` | 152 ノードの経路表の計算(既定の方式) | 合計が 16ms 以下 |
| `compute/RepeatedDijkstra/68` | 典型的な終盤 | 1ms 未満の見込み(記録だけ) |
| `compute/RepeatedDijkstra/256`、`build_next_hops/256` | 理論上の最大(v1 では起きない) | 記録だけ。予備案1はまだないので、SW-5 の判断には使わない(H9 に「予備案1を作るときに測り直す」と書く) |
| `compute/FloydWarshall/…` | 比べるための別の方式 | 記録だけ |
| `tick/average_full`、`tick/average_midgame` | 平均の 1 tick(再計算なし) | 2ms 以下(02 §9.2)。数 µs の見込み(11 §10.1) |
| `tick/worst_observed`、`tick/worst_all_arrive` | 最悪の 1 tick(4 本・48 本が同時に着く) | 2ms 以下。0.1ms 未満の見込み |
| `recommendation/full`、`recommendation/midgame` | おすすめの 1 回 | 0.1ms 程度の見込み。update の予算 1ms の中に収まる |

- 3B と開発機の比(`3B の値 ÷ 開発機の値`)を、`routes/recompute_full_tick` と `compute/RepeatedDijkstra/152` で計算して書く(12 §5.4 の仮定 15〜30 倍の確認。RT-V-01)。
- 12 §5.4 の 192 ノードのベンチマークはない。152 と 256 の値から判断する。
- 02 V6 の「aarch64 と armv7 の速度差」: armv7 のイメージは作っていないので測らない(H4 に「対象外」と書く)。

## 8. メモリと 8 時間の放置(開発用。02 §9.3、V16、PL-V-12、AS-V-10)

### 8.1 記録のスクリプトを置く

実機で、次をそのまま貼り付けて `/root/rss-log.sh` を作る(BusyBox のコマンドだけで動く。開発機の BusyBox で動くことを確かめた):

```sh
cat > /root/rss-log.sh <<'EOF'
#!/bin/sh
# 使い方: sh /root/rss-log.sh [間隔の秒数(既定 60)] > /root/rss.csv
i=${1:-60}
echo "uptime_s,pid,VmRSS_kB,VmHWM_kB,exe_Rss_kB,MemAvailable_kB"
while :; do
  u=$(cut -d . -f 1 /proc/uptime)
  p=$(pidof trainmaster)
  if [ -n "$p" ] && [ -r /proc/$p/status ]; then
    r=$(awk '/^VmRSS/ { print $2 }' /proc/$p/status)
    h=$(awk '/^VmHWM/ { print $2 }' /proc/$p/status)
    e=$(awk '/^[0-9a-f]+-[0-9a-f]+ / { f = ($NF == "/usr/bin/trainmaster") } f && /^Rss:/ { s += $2 } END { print s + 0 }' /proc/$p/smaps)
  else
    p=-; r=-; h=-; e=-
  fi
  a=$(awk '/^MemAvailable/ { print $2 }' /proc/meminfo)
  echo "$u,$p,$r,$h,$e,$a"
  sleep "$i"
done
EOF
```

| 列 | 意味 |
|---|---|
| `uptime_s` | 起動してからの秒数 |
| `pid` | ゲームのプロセスの番号。変わったら、ゲームが起動し直した(落ちた)ということ |
| `VmRSS_kB` | ゲームがいま使っているメモリ(KiB)。**予算 128MB(約 131,072KiB)と比べる** |
| `VmHWM_kB` | 起動してからいちばん多く使ったときの値 |
| `exe_Rss_kB` | 実行ファイル(音と絵のデータを含む)のうち、メモリに載っている分。8 時間たっても大きく減らなければ、音のページが追い出されていない(AS-V-10) |
| `MemAvailable_kB` | システム全体で、まだ使えるメモリ |

### 8.2 ためしに 1 分

```sh
sh /root/rss-log.sh 5 & sleep 30; kill $!
```

6 行ほど出て、`VmRSS_kB` に数字が入っていればよい。`-` ばかりなら、ゲームが動いていない(`pidof trainmaster` を見る)。

### 8.3 8 時間の放置

1. `/etc/default/trainmaster` に `TM_LOG=debug` があるかを `cat` で見る。なければ足して `killall trainmaster` する(§2.3)。音の命令を捨てた数(§8.4)をログで見るため。
2. ゲームを新しく始め、路線を 2〜3 本作ってお金が入る状態にする(放置で収入が増え続ける状態。02 §9.5)。会社レベルが上がっても説明は読んで閉じ、**ゲームを止める説明が出たままにしない**(止まっていると放置にならない)。
3. 端末 B で記録を始める(SSH を切っても止まらないように `nohup` を付け、止めるための番号を残す):
   ```sh
   nohup sh /root/rss-log.sh 60 > /root/rss.csv 2>/dev/null &
   echo $! > /root/rss-log.pid
   ```
4. そのまま 8 時間置く。途中で画面を見て、動いているか(列車が走っているか)だけ確かめてよい。キーは押さない。
5. 8 時間たったら、記録を止めて(実機で)、開発機に持ってくる(開発機で):
   ```sh
   kill $(cat /root/rss-log.pid)
   ```
   ```sh
   ssh root@192.168.1.23 cat /root/rss.csv > rss.csv
   ssh root@192.168.1.23 cat /tmp/trainmaster.log > game-8h.log
   ```

**8 時間たった後の操作**: キーで少し遊び、ふつうに動くか(遅くなっていないか、音が鳴るか)を見る。

### 8.4 まとめる(開発機で)

```sh
awk -F, 'NR > 1 && $3 != "-" { if (t0 == "") t0 = $1; t = $1 - t0; if (t >= 3600 && r1 == "") r1 = $3; if (t >= 3600 && $3 + 0 > m) m = $3 + 0; last = $3; n++ } END { print "行数", n, "1時間後", r1, "最後", last, "1時間後からの最大", m, "増え方(最後 - 1時間後)", last - r1 }' rss.csv
awk -F, 'NR > 2 && $2 != p { print "pid が変わった:", $0 } { p = $2 }' rss.csv
head -n 3 rss.csv; tail -n 3 rss.csv
grep -E '音: 捨てた命令|WARN|ERROR' game-8h.log | sort | uniq -c | head
```

- グラフにしたいときは、`rss.csv` を表計算ソフトで開き、`uptime_s` と `VmRSS_kB` で折れ線にする。
- **合格の目安**(14 §7.3 の「長時間」、02 §9.3):
  - `VmRSS_kB` の最大が 131,072 以下(128MB 以下)。
  - 最初の 1 時間より後で、RSS が増え続けない(「増え方」がほぼ 0。数百 KiB の上下はよい。1 時間ごとに増える階段の形なら不合格)。
  - `pid が変わった` の行がない(8 時間のあいだにゲームが落ちていない)。
  - ログに `音: 捨てた命令 …` の `warn` がない(音の命令を捨てていない)。
  - `exe_Rss_kB` が最初と比べて大きく減っていない(音のページが追い出されていない。AS-V-10)。

## 9. パッド・音の最終確認(BV8、UI-V-05、BV11・PL-V-10 の最終、PL-V-09・PL-V-16 の測り直し、UI-V-09)

### 9.1 展示のパッド(BV8、UI-V-05。V1 で機種が決まってから)

1. `scripts/av-poc-device.md` §5.2 の 1〜7 を、**展示のパッド**で行う(ゲームを止めて `av_poc` を使う)。結果は H6 に書く。
2. ゲームに戻し(`rm /run/trainmaster.hold`)、パッドだけで遊ぶ:

| 見ること | 合格の目安 |
|---|---|
| スティックに触らずに 1 分置いて、カーソルが勝手に動かないか(ドリフト) | 動かない |
| スティックを半分くらい倒したときに、カーソルが 1 マス動くか。ゆっくり倒して、動き始める所が自然か(閾値 16384、戻す閾値 12288。13 §2.5) | 半分で動き、指を離すと止まる |
| 斜めに倒したとき、思った向きに動くか | 迷わない |
| ボタンの役割(A で決定、B でもどる、Y で MAX、X で ろせん、LB/RB でタブ、Start で せってい、Back で おすすめ。13 §2.2)が画面のボタンの絵と合っているか | 合っている |

- 閾値が合わないと感じたら、「もっと倒さないと動かない方がいい / 少しで動く方がいい」のように方向を書く(直すのは 13 の担当)。

### 9.2 会場の表示機器の音(BV11、PL-V-10 の最終)

会場と同じ表示機器で、`scripts/av-poc-device.md` §6.1 の表の 1〜6 と 10 を行う(7〜9 はディスプレイを替える試験なので、会場の機器では要らない)。結果は H7 に書く。あわせて、ゲームに戻してタイトルの BGM とゲーム中の音を聞き、会場の機器のスピーカーで音が割れないか、小さすぎないかを見る(音量の聞きくらべは `scripts/listen-audio.md`)。

### 9.3 音の途切れと遅れの測り直し(PL-V-09、PL-V-16)

`scripts/av-poc-audio.md` の PL-V-09 の 1〜8 と PL-V-16 を、IMP-006 の後に変わった分(本番の音・最新のイメージ)で行う。読みかえは `scripts/av-poc-device.md` §6.2・§6.3 のとおり。結果は H7 に書く(IMP-006 の R6 と比べる)。

### 9.4 ジングルの終わりと説明の出る時刻(UI-V-09)

ゲームは、ジングルの長さから「鳴り終わる時刻」を計算し、それに 100ms ずつの余裕を足してから、ゲームを止める説明を出す(13 §5.3)。実際に鳴り終わる時刻とのずれを確かめる。

1. 新しいゲームを始め、スマートフォンで画面とスピーカーの音が入るように動画を撮る。
2. 会社 Lv1 に上がるところ(`jingle_company_up` が鳴り、駅の音 `se_station_new` の後に説明「新しい えきが できた!」が出る)を撮る。Lv4(`jingle_all_unlocked`)でももう 1 回撮る。
3. 開発機で動画を開き(コマ送りできるソフトか、音の波形が見えるソフト)、次の 3 つの時刻を読む:
   - ジングルの音が聞こえなくなった時刻
   - `se_station_new`(「ポン」ときらきら)が鳴り始めた時刻
   - 説明の箱が出た時刻
- **合格の目安**: ジングルが鳴り終わる前に `se_station_new` が鳴り始めない。説明はジングルと駅の音が終わってから出る。終わってから説明までが長すぎない(0.5 秒くらいまで)。

## 10. 画面の読みやすさ(大人が見る分。UI-V-01、UI-V-10、AS-V-04、AS-V-11、AS-V-13、V7)

子どもに見てもらう分は `scripts/playtest.md` で行う。ここでは、大人が会場と同じ表示機器と距離で、先に問題がないかを見る。

1. 会場と同じ表示機器に配布用でゲームを出す。子どもが座る(立つ)位置と同じ距離から見る。距離をメートルで書く。
2. 次を見て、H8 に「読める / 読みにくい」と、読みにくい字・場所を書く:

| 見ること | 何で見るか |
|---|---|
| 16px の文字(HUD・情報欄・強化パネル)と 24px の説明の文字が読めるか。特に画数の多い字(曜、議、穣、澗、数の単位) | ゲームの画面。数の単位はゲームが進まないと出ないので、下の 3. の画像も使う |
| マス(16px、画面では 48px)の駅・列車・線路が見分けられるか | ゲームの画面 |
| 線路の帯(3 本のレーン、斜めのずれ、曲がりのくびれ)で、同じ区間を何本の路線が通るかが分かるか(AS-V-13) | 路線を 2〜3 本、同じ駅を通るように作る |
| 色の見え方(文字と地の差、灰色にした駅、光るおすすめ。UI-V-10) | ゲームの画面 |
| 路線の 6 色の見分け(AS-V-11) | 路線が 6 本そろった画面か、下の 4. の画像 |

3. 開発機で、文字の試しの画像を作り、ディスプレイに全画面で出して同じ距離から見る(3 倍は画面での大きさと同じ):
   ```sh
   mise exec -- cargo run --locked -q -p tm-fontgen -- preview --face Body12 --scale 3 "曜 議 穣 澗 1無量大数円 みりょく 人気" -o font16.png
   mise exec -- cargo run --locked -q -p tm-fontgen -- preview --face Head16 --scale 3 "きみは 電車の 会社の 社長だ!" -o font24.png
   ```
4. 路線の 6 色の列車を並べた画像を作る(色は `crates/tm-ui/src/draw/palette.rs` の `LINES`):
   ```sh
   mise exec -- cargo run --locked -q -p tm-spritegen -- preview trn_e:E69F00@0,0 trn_e:56B4E9@16,0 trn_e:009E73@32,0 trn_e:F0E442@48,0 trn_e:0072B2@64,0 trn_e:D55E00@80,0 --scale 6 -o lines.png
   ```
   この画像(か、ゲームの画面を撮った写真)を、色覚の模擬ができるツール(スマートフォンのアプリなど。使ったものの名前を書く)に通し、6 色が見分けられるかを見る。

- **合格の目安**: 大人が会場の距離で、文字をすべて読め、路線の数と色が分かる。読みにくいものは、直す方向(「本文を 20px に」「この 2 色が似ている」など)を書いて、`scripts/playtest.md` で子どもにも確かめる。

## 11. うまくいかないとき

- **ゲームが起動しない・すぐ落ちる**: `cat /tmp/trainmaster.log /tmp/trainmaster.log.1` と `dmesg | tail -n 30` を写す。描画と音の切り分けは `scripts/av-poc-device.md` §8・§9。
- **`killall trainmaster` の後に起動し直さない**: `ls /run/trainmaster.hold /etc/trainmaster.hold` で hold のファイルが残っていないか見る。
- **ベンチマークが `not found` や `No such file` で動かない**: 7.1 で Buildroot のリンカを指定し忘れている(開発機の別の glibc にリンクされた)。`CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER` を付けてビルドし直す。
- **8 時間の記録が途中で止まった**: `pidof trainmaster` と `uptime` を見る。再起動していたら(`uptime` が短い)電源を見直してやり直す。

---

## 記録の表

数字はログの値をそのまま書く。できなかったものは「できない」と、その理由を書く。

### H0 共通

| 項目 | 結果 |
|---|---|
| 記録した人 | |
| イメージ(配布用・開発用)のゲームの版(ログの 1 行目 `trainmaster …`) | |
| ディスプレイの機種(会場と同じか) | |
| ゲームパッドの機種(展示用か、仮か) | |
| 電源(A 数) | |

### H1 起動の時間と大きさ(§3)

| 項目 | 1 回目 | 2 回目 | 3 回目 | 合否 |
|---|---|---|---|---|
| 電源からタイトル画面まで(秒。動画) | | | | |
| `tm-launch: starting trainmaster at …s` | | | | — |
| `SdlCore::new` / `SdlPlatform::new`(ms) | | | | — |
| `音のページの読み込み`(ms) | | | | — |
| `load_assets`(ms) | | | | — |
| `最初のフレーム` の行の頭の秒数(プロセスの起動から最初の表示。3 秒以内) | | | | |

| 項目 | 結果 | 合否 |
|---|---|---|
| `sdcard.img`・`rootfs.ext4`・`boot.vfat`・`trainmaster` の大きさ | | — |
| SD の boot と rootfs の `df -h`(配布用) | | |
| 開発用の `df -h /`、boot の空き、`/proc/cmdline`、`/proc/device-tree/model`、`/proc/uptime` | | — |
| HDMI に文字が出ない・カーソルが点滅しない・15 分で画面が消えない | | |
| blacklist の 7 つが読み込まれていない | | |
| blacklist あり / なしの `at …s`(各 2 回) | | — |
| `crng init done` の時刻、Enter からゲームの画面までの待ち(V13) | | |

| 候補の比較 | 電源からタイトル(秒、3 回) | `at …s`(3 回) | シリアル | 音 | HDMI | 採るか(提案) |
|---|---|---|---|---|---|---|
| A いまのまま | | | | | | — |
| B `boot_delay=0` | | | | | | |
| C `disable-bt` + `disable-wifi` | | | | | | |
| D B + C | | | | | | |

### H2 シリアルと rootfs(§4)

| 項目 | 結果 | 合否 |
|---|---|---|
| 開発用: `trainmaster login:` がシリアルに出る | | |
| `ls /dev/ttyAMA*`、`dmesg \| grep tty` の行(BV29) | | |
| `/proc/consoles` | | |
| HDMI の遅れ(`defer` の行)、vc4 の行(BV6) | | |
| 配布用: シリアルに `Starting …`、`tm-launch: …`、ゲームのログが出る(BV29) | | |
| 配布用: `fail\|error\|read-only…` で出た行(すべて写す。BV12) | | |
| 配布用: キャッシュを書けない行(BV30) | | |

### H3 tm-launch とフレームの重さ(§5、§6)

| tm-launch(§5) | 結果(シリアルの行をそのまま) | 合否 |
|---|---|---|
| 1 10 秒以上の後の異常終了 → すぐ起動 | | |
| 2 続けて落ちたときの待ち(5 回分の秒数) | | |
| 3 待ちが 0 に戻る | | |
| 4 hold で起動しない | | |
| 5 hold を消すと起動する(秒) | | |
| 6 書きまちがいの行がシリアルにだけ出る | | |
| HDMI に文字が出ない・シリアルが乱れない・ログが同じ(`tee`) | | |
| `XDG_CACHE_HOME`、`/root/.cache` の有無 | | |

| 場面 | input 平均/最大 | update 平均/最大 | sim 平均/最大 | render 平均/最大 | step の分布 | 間隔の平均 | 捨てた tick | 四角形 | 合否 |
|---|---|---|---|---|---|---|---|---|---|
| T タイトル | | | | | | | | | |
| E 序盤 | | | | | | | | | |
| M 中盤 | | | | | | | | | |
| L 終盤(路線の数、線路のマスの目安) | | | | | | | | | |
| C `copy`(場面: ) | | | | | | | | | — |
| F30 30fps(場面: ) | | | | | | | | | |

| 項目 | 結果 |
|---|---|
| 路線を作る・のばす・レベルが上がるときのカクつき(目で見て) | |
| `step の回数 …、間隔のずれ …` の行(APP-V-02) | |
| `WARN`・`ERROR` の行 | |

### H4 ベンチマーク(§7。`time:` のまん中の値)

| ベンチマーク | 3B | 開発機 | 比(3B ÷ 開発機) | 合否 |
|---|---|---|---|---|
| `routes/recompute_full_tick` | | | | |
| `compute/RepeatedDijkstra/68` | | | | — |
| `compute/RepeatedDijkstra/152` | | | | |
| `build_next_hops/152` | | | | |
| `compute/RepeatedDijkstra/256` | | | | — |
| `build_next_hops/256` | | | | — |
| `compute/FloydWarshall/68・152・256` | | | — | — |
| `tick/average_full` | | | | |
| `tick/average_midgame` | | | | |
| `tick/worst_observed` | | | | |
| `tick/worst_all_arrive` | | | | |
| `recommendation/full` | | | | |
| `recommendation/midgame` | | | | |
| 始める前と後の温度 | | — | — | — |
| armv7 との差(V6) | 対象外(armv7 のイメージはない) | | | — |

### H5 メモリと 8 時間(§8)

| 項目 | 結果 | 合否 |
|---|---|---|
| 放置を始めたときの状態(会社レベル、路線の数) | | — |
| `VmRSS` の最初・1 時間後・最後・最大(KiB) | | |
| 増え方(最後 − 1 時間後) | | |
| `VmHWM` の最後 | | — |
| `exe_Rss` の最初・最後 | | |
| `MemAvailable` の最初・最後 | | — |
| `pid が変わった` の行 | | |
| `音: 捨てた命令 …` の行 | | |
| 8 時間の後に遊んで、ふつうに動くか | | |

### H6 パッド(§9.1)

| 項目 | 結果 | 合否 |
|---|---|---|
| `scripts/av-poc-device.md` §5.2 の 1〜7 を展示のパッドで(R5 と同じ項目をここに) | | |
| ドリフト | | |
| 半分倒したときの動き、閾値の感じ(直す方向) | | |
| 斜め | | |
| ボタンの役割と絵 | | |

### H7 音の最終確認(§9.2〜§9.4)

| 項目 | 結果 | 合否 |
|---|---|---|
| 会場の機器: `Starting audio: …` の行、`/run/asound.conf` | | |
| 会場の機器: aplay の音、`音のデバイス(alsa): …` の行 | | |
| 会場の機器: BGM と効果音、雑音、割れ | | |
| 3.5mm: 鳴るか、`PCM` の dB | | |
| null: デバイスが開ける | | |
| PL-V-09(1024・2048): `late_callbacks`、`max_callback_gap_us`、捨てた数、`%CPU`、`audio() 最大`、耳 | | |
| PL-V-16(1024・2048): 10 回の最大(ms) | | |
| UI-V-09 Lv1: ジングルの終わり → `se_station_new` → 説明(秒) | | |
| UI-V-09 Lv4: 同じ | | |

### H8 画面の読みやすさ(大人。§10)

| 項目 | 結果(読みにくい字・場所、直す方向) | 合否 |
|---|---|---|
| 見た距離(m)、表示機器 | | — |
| 16px の文字 | | |
| 24px の説明 | | |
| 画数の多い字(`font16.png`) | | |
| マスの駅・列車・線路 | | |
| 帯で路線の数が分かるか(AS-V-13) | | |
| 色の見え方(UI-V-10) | | |
| 6 色の見分け(実機 / 模擬。使ったツール) | | |

### H9 まとめ(メインエージェントへ。IMP-504 の入力)

| 項目 | 結果 |
|---|---|
| 予算を超えた項目(節、測った値、予算) | |
| 起動の候補で採るもの(BV9) | |
| 既定を変える案(`Copy`、30fps、720p など) | |
| 経路の計算の判断(12 §5.6 の SW-1〜SW-5 のどれに当たるか) | |
| パッドの閾値・マッピングで直すこと | |
| そのほか気づいたこと | |
