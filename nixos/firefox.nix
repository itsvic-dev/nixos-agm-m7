{ pkgs, ... }:

let
  # Changing only passthru keeps the cached Firefox build and drops the media stack from the wrapper.
  unwrapped = pkgs.firefox-unwrapped.overrideAttrs (old: {
    passthru = old.passthru // {
      ffmpegSupport = false;
      pipewireSupport = false;
      alsaSupport = false;
      jackSupport = false;
      sndioSupport = false;
      gssSupport = false;
    };
  });

  firefox = pkgs.wrapFirefox unwrapped {
    extraPrefs = ''
      pref("layout.css.devPixelsPerPx", "0.6");
      pref("browser.uidensity", 1);
      pref("browser.startup.homepage", "https://lite.duckduckgo.com/lite/");
      pref("fission.autostart", false);
      pref("dom.ipc.processCount", 1);
      pref("browser.tabs.unloadOnLowMemory", true);
      pref("browser.cache.memory.capacity", 16384);
      pref("gfx.webrender.software", true);
      pref("browser.ml.enable", false);
      pref("browser.ml.chat.enabled", false);
      pref("extensions.pocket.enabled", false);
      pref("browser.newtabpage.activity-stream.feeds.topsites", false);
      pref("browser.newtabpage.activity-stream.feeds.section.topstories", false);
      pref("toolkit.telemetry.enabled", false);
      pref("datareporting.healthreport.uploadEnabled", false);
      pref("datareporting.policy.dataSubmissionEnabled", false);
    '';
  };
in
{
  environment.systemPackages = [ firefox ];

  zramSwap.enable = true;
}
