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

  # Firefox's toolbars ask for more than the 240 px wide screen; drop the tab strip and minimum widths.
  chrome = ''
    #TabsToolbar, #sidebar-main, #sidebar-box { visibility: collapse !important; }
    #nav-bar, #urlbar-container, #urlbar, #search-container, toolbar, toolbaritem, toolbarbutton {
      min-width: 0 !important;
    }
    #urlbar-container { flex: 1 1 0 !important; }
    #main-window, #browser, #tabbrowser-tabbox, browser { min-width: 0 !important; min-height: 0 !important; }
  '';

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
      pref("ui.prefersReducedMotion", 1);
      pref("toolkit.cosmeticAnimations.enabled", false);
      pref("general.smoothScroll", false);
      pref("layout.frame_rate", 30);
      pref("network.prefetch-next", false);
      pref("network.predictor.enabled", false);
      pref("browser.startup.preXulSkeletonUI", false);

      try {
        const sss = Components.classes["@mozilla.org/content/style-sheet-service;1"]
          .getService(Components.interfaces.nsIStyleSheetService);
        const io = Components.classes["@mozilla.org/network/io-service;1"]
          .getService(Components.interfaces.nsIIOService);
        sss.loadAndRegisterSheet(io.newURI("data:text/css," + encodeURIComponent(${builtins.toJSON chrome})),
          sss.USER_SHEET);
      } catch (e) {
        Components.utils.reportError(e);
      }
    '';
  };
in
{
  environment.systemPackages = [ firefox ];

  zramSwap.enable = true;
}
