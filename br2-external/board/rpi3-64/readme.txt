Raspberry Pi 3 Model B: イメージを作って実機で起動する手順

コマンドはすべてリポジトリのいちばん上のディレクトリ(mise.toml がある場所)で打つ。

0. はじめに一度だけ用意するもの

   - Ubuntu か Debian の開発機。ディスクの空きは 1 つのイメージにつき約 5GB
     (出力が約 4.7GB、ダウンロードが約 1GB。2 つ作ると約 10GB)
   - OS のパッケージ(Buildroot が使う道具):
       sudo apt-get install -y build-essential diffutils patch gzip bzip2 perl tar cpio \
         unzip rsync file bc findutils gawk wget curl git python3 xz-utils libncurses-dev
   - mise の道具: mise trust && mise install
   - 出力先を変えたいときは、リポジトリのいちばん上に mise.local.toml を作る(git には入らない):
       [env]
       BR_OUTPUT = "/home/あなた/.cache/br-output/rpi3-64"
       BR2_DL_DIR = "/home/あなた/.cache/br-dl"
     このときは下のパスも、build/rpi3-64 を BR_OUTPUT の値に読みかえる

1. イメージを作る

   mise run br:build-dev   # 開発用。できるもの: build/rpi3-64-dev/images/sdcard.img
   mise run br:build       # 配布用。できるもの: build/rpi3-64/images/sdcard.img(家用)と
                           #   build/rpi3-64/images/sdcard-exhibit.img(展示用)

   初めては約 30 分かかる(16 コアのとき。インターネットからソースを取ってくる)。2 回目からは速い。
   開発用と配布用は名前が同じ sdcard.img なので、どちらのディレクトリのものかを必ず確かめてから書く。

   家用と展示用のちがい(中身は同じで、ブートの cmdline.txt の最後の trainmaster.save=off だけがちがう):
     家用 sdcard.img:           遊んでいる会社と設定を SD カードの data パーティションに保存する。
                                電源を入れ直すとタイトルに「続きから」が出る。
     展示用 sdcard-exhibit.img: 保存しない。交代のたびにリセットする展示会で使う。
   開発用の sdcard.img は家用と同じく保存する。
   どのイメージも SD カードは 3 つのパーティション(boot、rootfs、data)になる。data(ext4、64MiB、
   ラベル TMDATA)は起動のたびに検査して /data にマウントし、壊れていたら作り直す(セーブは消える)。

   どちらも電源を入れると HDMI の画面(tty1)でゲームが起動し、終了・異常終了したら起動し直す。
   開発用: シリアルでログインできる。ネットワーク(DHCP)と SSH あり。書き込みできる。
           確認用の av_poc と stress-ng が入っている。描画・入力・音の確かめ方は scripts/av-poc-device.md
           (av_poc を動かす前に、下の 4 の「ゲームを止める」をする)。
   配布用: ログインはできない。ネットワークなし。書き込みできない(電源を急に切っても壊れにくい)。

   アプリや rootfs-overlay だけを直したとき(数分):
     配布用: mise run br:rebuild-app
     開発用: mise run br:cargo-fetch && make -C build/rpi3-64-dev trainmaster-rebuild all
   Cargo.lock を変えたとき: mise run br:rust-licenses で
     br2-external/package/trainmaster/THIRD-PARTY-LICENSES.txt を作り直してコミットする(CI が古さを検査する)
   config.txt や cmdline.txt を直したとき(br:build では入らない。展示用の cmdline.txt も作り直される):
     make -C build/rpi3-64 rpi-firmware-reinstall all(開発用は build/rpi3-64-dev)
   配布物(家用と展示用の 2 つの .img.xz など)を作る: mise run br:dist(build/dist にできる)
     未コミットの変更があると止まる。試しに作るときは ALLOW_DIRTY=1 mise run br:dist
     確かめ方: cd build/dist && sha256sum -c SHA256SUMS
       書き込む前の .img は xz -dc trainmaster-*-rpi3-arm64.img.xz | sha256sum の値を
       BUILDINFO.txt の "sha256 of ... .img" の行と比べる(展示用は -rpi3-arm64-exhibit.img.xz)

   実機の前に QEMU でも試せる: mise run qemu:rpi3 -- --dev(終わるときは Ctrl-A を押してから X)

2. SD カードに書く(SD カードの中身はすべて消える。保存していた会社と設定も消える)

   家で遊ぶなら家用(sdcard.img、配布物では *-rpi3-arm64.img.xz)、展示会なら展示用
   (sdcard-exhibit.img、*-rpi3-arm64-exhibit.img.xz)を書く。
   同じカードで家用と展示用を入れ替えるときも、新しい版に上げるときも、セーブは消える。
   書いたあとで PC から boot の cmdline.txt を直して切り替えてもよい
   (行の最後に " trainmaster.save=off" があれば保存しない)。

   Raspberry Pi Imager を使うとき:
     「デバイスを選ぶ」で Raspberry Pi 3、「OS を選ぶ」でいちばん下の「カスタムイメージを使う」から
     sdcard.img、「ストレージを選ぶ」で SD カードを選ぶ。
     「OS のカスタマイズを使いますか」には「いいえ」を選ぶ(使うとブートの設定が書き換わる)。

   dd を使うとき(書き先をまちがえると開発機のディスクが消えるので、よく確かめる):
     lsblk                          # SD カードを差す前と後で比べ、増えたもの(例 /dev/sdb)が SD カード
     sudo umount /dev/sdb?*         # 自動でマウントされていたら外す(/dev/sdb は自分の値に)
     sudo dd if=build/rpi3-64-dev/images/sdcard.img of=/dev/sdb bs=4M conv=fsync status=progress
     sync                           # 終わってから SD カードを抜く

3. シリアルをつなぐ(3.3V の USB シリアル変換アダプタを使う。5V のものはラズパイを壊す)

   ラズパイの GPIO のピン 6 (GND)   ― アダプタの GND
   ラズパイの GPIO のピン 8 (TXD)   ― アダプタの RXD
   ラズパイの GPIO のピン 10 (RXD)  ― アダプタの TXD
   アダプタの VCC(3.3V や 5V)のピンは、どこにもつながない。

   開発機で:
     sudo usermod -aG dialout $USER  # 一度だけ。Permission denied になるとき。ログインし直すと効く
     picocom -b 115200 /dev/ttyUSB0  # アダプタによっては /dev/ttyACM0。終わるときは Ctrl-A を押してから Ctrl-X
   (screen /dev/ttyUSB0 115200 でもよい)

4. 電源を入れる

   電源は 5V 2.5A 以上のものを使う。

   HDMI の画面にゲームが出る。シリアルには起動スクリプトの "Starting ..." の行と、ゲームのログが出る。
   data パーティションのマウントは "data: mounted (e2fsck 0, 0.12s)" のような行で出る
   (", journal replayed" は前回は電源をいきなり切られた、", recreated" は壊れていたので作り直してセーブが消えた、
   "data: not mounted, saving is off" などは保存なしで遊ぶ)。
   遊び終わるときは「せってい」の「ゲームを終わる」を選び、画面が消えてから電源を抜く
   (シリアルには "tm-launch: power off requested" と "reboot: Power down" が出る)。
   途中で電源を抜いても、最後の保存(ゲームの時間で最大 1 分前)から続けられる。
   ゲームのログは /tmp/trainmaster.log(1 つ前の起動のものは /tmp/trainmaster.log.1)にもある。
   ゲームがすぐ(10 秒未満で)終わる回が続くと、起動し直す前に 2、4、8、16 秒待つ(16 秒より長くは待たない)。
   開発用: シリアルに "trainmaster login:" と出たら root と打つ(パスワードなし)。
     SSH を使うときは、先に passwd で root のパスワードを決める。
     ゲームを止める: touch /run/trainmaster.hold; killall trainmaster
       (再び動かす: rm /run/trainmaster.hold。電源を入れ直しても止めておくなら /etc/trainmaster.hold)
     環境変数を付けて試す: /etc/default/trainmaster に TM_... を書いて killall trainmaster

5. 記録すること(BV9)

   - 電源を入れてから、シリアルに "tm-launch: starting trainmaster" が出るまでと、タイトル画面が出るまでの秒数
   - シリアルの "data: mounted (e2fsck <rc>, <秒>s)" の行(BV32)
   - ログインしたら: cat /proc/uptime、cat /proc/cmdline、cat /proc/device-tree/model
   - rootfs の大きさ: df -h /
   - boot の空き: mount /dev/mmcblk0p1 /mnt && df -h /mnt && umount /mnt
   - HDMI の画面にカーネルのメッセージが出ないか、カーソルが点滅しないか、放置して画面が消えないか

6. うまくいかないとき

   - 何も起きない: 緑の ACT LED が点滅しなければ SD カードを読めていない。書き込みをやり直す。
     赤い LED が消えたり点滅したりするなら電源が足りない。
   - シリアルに何も出ない: 線の向き(TXD と RXD は交差させる)と GND を確かめる。
     開発用なら SSH でログインして ls /dev/ttyAMA* と dmesg | grep tty を見る
     (miniuart-bt が効かないと ttyAMA0 ではなく ttyAMA1 になる)。
   - 文字化けする: 速度が 115200 か、GND がつながっているかを確かめる。
   - 開発用で起動が途中で止まったように見える: LAN ケーブルや DHCP のサーバーがないと、
     ネットワークの設定で十数秒待つ。FAIL と出ても、そのあとログインできれば問題ない。
