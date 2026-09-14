#!/bin/bash
# Clone plan generated from references/repositories.json (2026-09-15).
# 63 of 64 entries; R41 (AOSP frameworks/av) excluded: entry unverified, googlesource mega-repo.
# Shallow single-branch clone (default branch, depth 1) with working tree, for read-only reference.
# No submodule init; existing dirs skipped.
set -u
ROOT="/Volumes/Portable2TB/ExtDev/others"
cd "$ROOT" || exit 1
clone() { slug="$1"; url="$2";
  if [ -d "$slug" ]; then echo "SKIP $slug (exists)"; return 0; fi
  echo "CLONE $slug";
  git -c protocol.ext.allow=never -c protocol.file.allow=never clone --depth=1 --single-branch --no-recurse-submodules --quiet "$url" "$slug" || echo "FAIL $slug"
}
clone r01-uxplay https://github.com/FDH2/UxPlay.git
clone r02-shairplay-rust https://github.com/metaneutrons/shairplay-rust.git
clone r03-rpiplay https://github.com/FD-/RPiPlay.git
clone r04-android-airplay-server https://github.com/jqssun/android-airplay-server.git
clone r05-shairplay https://github.com/juhovh/shairplay.git
clone r06-playfair https://github.com/EstebanKubata/playfair.git
clone r07-shairport-sync https://github.com/mikebrady/shairport-sync.git
clone r08-airplayreceiver https://github.com/SteeBono/airplayreceiver.git
clone r09-airplay2onwindows https://github.com/YimingZhanshen/Airplay2OnWindows.git
clone r10-airplay-spec https://github.com/openairplay/airplay-spec.git
clone r11-pyatv https://github.com/postlund/pyatv.git
clone r12-airconnect https://github.com/philippe44/AirConnect.git
clone r13-protocol https://github.com/localsend/protocol.git
clone r14-localsend https://github.com/localsend/localsend.git
clone r15-neardrop https://github.com/grishka/NearDrop.git
clone r16-bada https://github.com/kyujin-cho/Bada.git
clone r17-nearby https://github.com/google/nearby.git
clone r18-ukey2 https://github.com/google/ukey2.git
clone r19-rquickshare https://github.com/Martichou/rquickshare.git
clone r20-qnearbyshare https://github.com/vicr123/QNearbyShare.git
clone r21-opendrop https://github.com/seemoo-lab/opendrop.git
clone r22-owl https://github.com/seemoo-lab/owl.git
clone r23-android https://github.com/nearby-sharing/android.git
clone r24-kdeconnect-kde https://github.com/KDE/kdeconnect-kde.git
clone r25-gnome-shell-extension-gsconnect https://github.com/GSConnect/gnome-shell-extension-gsconnect.git
clone r26-pairdrop https://github.com/schlagmichdoch/PairDrop.git
clone r27-snapdrop https://github.com/SnapDrop/snapdrop.git
clone r28-magic-wormhole https://github.com/magic-wormhole/magic-wormhole.git
clone r29-magic-wormhole.rs https://github.com/magic-wormhole/magic-wormhole.rs.git
clone r30-croc https://github.com/schollz/croc.git
clone r31-syncthing https://github.com/syncthing/syncthing.git
clone r32-miraclecast https://github.com/albfan/miraclecast.git
clone r33-gnome-network-displays https://github.com/GNOME/gnome-network-displays.git
clone r34-miracast-sink https://github.com/ivygroup/miracast-sink.git
clone r35-universal-miracast-sink https://github.com/FoxLost/universal-miracast-sink.git
clone r36-miracastreceiver https://github.com/weekdayjast/MiracastReceiver.git
clone r37-castengine_wifi_display https://github.com/openharmony/castengine_wifi_display.git
clone r38-castengine_cast_framework https://github.com/openharmony/castengine_cast_framework.git
clone r39-castengine_cast_plus_stream https://github.com/openharmony/castengine_cast_plus_stream.git
clone r40-communication_dsoftbus https://github.com/openharmony/communication_dsoftbus.git
clone r42-openscreen https://chromium.googlesource.com/openscreen
clone r43-castreceiver https://github.com/googlecast/CastReceiver.git
clone r44-pychromecast https://github.com/home-assistant-libs/pychromecast.git
clone r45-protocol https://github.com/cast-web/protocol.git
clone r46-pupnp https://github.com/pupnp/pupnp.git
clone r47-platinum https://github.com/plutinosoft/Platinum.git
clone r48-gerbera https://github.com/gerbera/gerbera.git
clone r49-rygel https://github.com/GNOME/rygel.git
clone r50-mediamtx https://github.com/bluenviron/mediamtx.git
clone r51-gortsplib https://github.com/bluenviron/gortsplib.git
clone r52-gstreamer https://github.com/GStreamer/gstreamer.git
clone r53-gstreamer-rs https://github.com/GStreamer/gstreamer-rs.git
clone r54-webrtc https://github.com/webrtc-rs/webrtc.git
clone r55-srt https://github.com/Haivision/srt.git
clone r56-scrcpy https://github.com/Genymobile/scrcpy.git
clone r57-sunshine https://github.com/LizardByte/Sunshine.git
clone r58-moonlight-common-c https://github.com/moonlight-stream/moonlight-common-c.git
clone r59-obs-studio https://github.com/obsproject/obs-studio.git
clone r60-ffmpeg https://github.com/FFmpeg/FFmpeg.git
clone r61-bluez https://github.com/bluez/bluez.git
clone r62-mdns-sd https://github.com/keepsimple1/mdns-sd.git
clone r63-btleplug https://github.com/deviceplug/btleplug.git
clone r64-windows-rs https://github.com/microsoft/windows-rs.git
echo ALL-DONE
