/**
 * Инициализация переключателя цветовой схемы.
 * Работает с:
 *  - <meta name="color-scheme">
 *  - <link id="css-light"> и <link id="css-dark">
 *  - <div id="scheme-switcher"> с кнопками data-scheme="light|dark|auto"
 */
export function initColorSchemeSwitcher() {
  const meta = document.querySelector('meta[name="color-scheme"]');
  const linkLight = document.getElementById("css-light");
  const linkDark = document.getElementById("css-dark");
  const switcher = document.getElementById("scheme-switcher");

  if (!linkLight || !linkDark) {
    console.warn("Color scheme links not found");
    return;
  }

  function applyScheme(scheme) {
    if (scheme === "light") {
      linkLight.media = "all";
      linkDark.media = "not all";
      meta && (meta.content = "light");
    } else if (scheme === "dark") {
      linkLight.media = "not all";
      linkDark.media = "all";
      meta && (meta.content = "dark");
    } else {
      linkLight.media = "(prefers-color-scheme: light)";
      linkDark.media = "(prefers-color-scheme: dark)";
      meta && (meta.content = "light dark");
    }
    localStorage.setItem("scheme", scheme);
  }

  const saved = localStorage.getItem("scheme") || "auto";
  applyScheme(saved);

  if (switcher) {
    switcher.querySelectorAll("button[data-scheme]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const scheme = btn.dataset.scheme;
        if (!scheme) return;
        switcher
          .querySelectorAll("button[data-scheme]")
          .forEach((b) => b.setAttribute("aria-pressed", String(b === btn)));
        applyScheme(scheme);
      });
    });
  }
}
