{ lib, pkgs, ... }:

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
    #urlbar-container { flex: 1 1 0 !important; margin-inline: 2px !important; }
    #forward-button, #home-button, #downloads-button, #fxa-toolbar-menu-button,
    #unified-extensions-button, #library-button, #sidebar-button, #firefox-view-button,
    #alltabs-button, #save-to-pocket-button, #nav-bar toolbarspring,
    #tracking-protection-icon-container, #identity-permission-box, #page-action-buttons,
    #star-button-box, #reader-mode-button, #translations-button {
      display: none !important;
    }
    #main-window, #browser, #tabbrowser-tabbox, browser { min-width: 0 !important; min-height: 0 !important; }
  '';

  # From postmarketOS mobile-config-firefox: add "Mobile;" so that sites send mobile pages, and fall back for sites that break with it.
  version = "${lib.versions.major unwrapped.version}.0";
  uaMobile = "Mozilla/5.0 (X11; Linux x86_64; Mobile; rv:${version}) Gecko/20100101 Firefox/${version}";
  uaDesktop = "Mozilla/5.0 (X11; Linux x86_64; rv:${version}) Gecko/20100101 Firefox/${version}";
  uaAndroid = "Mozilla/5.0 (Android 16; Mobile; rv:${version}) Gecko/${version} Firefox/${version}";
  uaRules = [
    [ "^https?://accounts[.]google[.]com($|/)" uaDesktop ]
    [ "^https?://(www[.])?google[.]com/maps($|/)" uaDesktop ]
    [ "^https?://maps[.]google[.]com($|/)" uaDesktop ]
    [ "^https?://(www[.])?google[.]com($|/)" uaAndroid ]
    [ "^https?://accounts[.]firefox[.]com($|/)" uaDesktop ]
    [ "^https?://addons[.]mozilla[.]org($|/)" uaDesktop ]
  ];

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
      pref("general.useragent.override", ${builtins.toJSON uaMobile});

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

      try {
        const uaRules = ${builtins.toJSON uaRules}.map(([re, ua]) => [new RegExp(re), ua]);
        Components.classes["@mozilla.org/observer-service;1"]
          .getService(Components.interfaces.nsIObserverService)
          .addObserver({
            observe(subject) {
              const channel = subject.QueryInterface(Components.interfaces.nsIHttpChannel);
              const rule = uaRules.find(([re]) => re.test(channel.URI.spec));
              if (rule) channel.setRequestHeader("User-Agent", rule[1], false);
            },
          }, "http-on-modify-request");
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
