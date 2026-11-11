################################################################################
#
# trainmaster
#
################################################################################

TRAINMASTER_VERSION = $(shell git -C $(BR2_EXTERNAL_TRAINMASTER_PATH)/.. describe --tags --always --dirty 2>/dev/null || echo unknown)
TRAINMASTER_SITE = $(BR2_EXTERNAL_TRAINMASTER_PATH)/..
TRAINMASTER_SITE_METHOD = local
# アプリのライセンスは未決定(LICENSE は雛形。03 §6.4)
TRAINMASTER_LICENSE = TBD (LICENSE), OFL-1.1 (font, NOTICE), Unicode-3.0 (kanji list, NOTICE)
TRAINMASTER_LICENSE_FILES = LICENSE NOTICE assets/fonts/OFL.txt
# legal-info は local のソースを build/ や target/ ごと tar にするので再配布させない。ソースは make-dist.sh が git archive で作る(03 §6.4)
TRAINMASTER_REDISTRIBUTE = NO
TRAINMASTER_DEPENDENCIES = sdl2

TRAINMASTER_OVERRIDE_SRCDIR_RSYNC_EXCLUSIONS = \
	--exclude /target --exclude /build --exclude /docs --exclude /.github

TRAINMASTER_CARGO_BUILD_OPTS = -p tm-app --bin trainmaster

# install は $(@D)/target から取るので、環境の CARGO_TARGET_DIR に左右されないようにする
TRAINMASTER_CARGO_ENV = \
	CARGO_TARGET_DIR=$(TRAINMASTER_DIR)/target \
	CARGO_PROFILE_RELEASE_OPT_LEVEL=3 \
	CARGO_PROFILE_RELEASE_LTO=true \
	CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
	CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C target-cpu=cortex-a53 \
		--remap-path-prefix=$(TRAINMASTER_DIR)=/build/trainmaster \
		--remap-path-prefix=$(BR_CARGO_HOME)=/cargo" \
	TM_BUILD_VERSION=$(TRAINMASTER_VERSION) \
	TM_RUSTC_VERSION="$$($(HOST_DIR)/bin/rustc --version)"

# 仮想ワークスペースのルートでは既定の "cargo install --path ./" が失敗するので、自前でコピーする
TRAINMASTER_PROFILE_DIR = $(if $(BR2_ENABLE_DEBUG),debug,release)
define TRAINMASTER_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 \
		$(@D)/target/$(RUSTC_TARGET_NAME)/$(TRAINMASTER_PROFILE_DIR)/trainmaster \
		$(TARGET_DIR)/usr/bin/trainmaster
	$(INSTALL) -D -m 0644 $(@D)/assets/fonts/OFL.txt \
		$(TARGET_DIR)/usr/share/licenses/trainmaster/BIZUDGothic-OFL.txt
	$(INSTALL) -D -m 0644 $(@D)/NOTICE \
		$(TARGET_DIR)/usr/share/licenses/trainmaster/NOTICE
	$(INSTALL) -D -m 0644 $(TRAINMASTER_PKGDIR)/THIRD-PARTY-LICENSES.txt \
		$(TARGET_DIR)/usr/share/licenses/trainmaster/THIRD-PARTY-LICENSES.txt
	$(foreach f,COPYRIGHT LICENSE-MIT LICENSE-APACHE rustc/share/doc/rust/COPYRIGHT-library.html, \
		$(INSTALL) -D -m 0644 $(HOST_RUST_BIN_DIR)/$(f) \
			$(TARGET_DIR)/usr/share/licenses/trainmaster/rust-std/$(notdir $(f))$(sep))
endef

ifeq ($(BR2_PACKAGE_TRAINMASTER_AV_POC),y)
define TRAINMASTER_BUILD_AV_POC
	cd $(TRAINMASTER_SRCDIR) && \
	$(TARGET_MAKE_ENV) $(TARGET_CONFIGURE_OPTS) $(PKG_CARGO_ENV) $(TRAINMASTER_CARGO_ENV) \
		cargo build --offline --release --locked --manifest-path Cargo.toml \
			-p tm-platform-sdl --example av_poc
endef
TRAINMASTER_POST_BUILD_HOOKS += TRAINMASTER_BUILD_AV_POC
define TRAINMASTER_INSTALL_AV_POC
	$(INSTALL) -D -m 0755 \
		$(@D)/target/$(RUSTC_TARGET_NAME)/release/examples/av_poc \
		$(TARGET_DIR)/usr/bin/av_poc
endef
TRAINMASTER_POST_INSTALL_TARGET_HOOKS += TRAINMASTER_INSTALL_AV_POC
endif

$(eval $(cargo-package))
