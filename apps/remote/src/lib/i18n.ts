import { useSyncExternalStore } from "react";
import { KEYS, safeStorage } from "./safeStorage";

/** Tiny i18n (es default, en secondary): the remote must stay small, so no i18next here. */
export type Lang = "es" | "en";

const es = {
  appName: "HollyShow Remoto",
  // pairing
  "pair.title": "Conecta tu celular",
  "pair.hint":
    "Escribe el PIN de 6 dígitos que aparece en la pantalla del operador, o escanea su código QR.",
  "pair.pin": "PIN",
  "pair.connect": "Conectar",
  "pair.checking": "Comprobando…",
  "pair.follow": "Seguir el culto sin código",
  "pair.err.invalid": "PIN incorrecto o caducado.",
  "pair.err.locked": "Demasiados intentos. Espera un momento.",
  "pair.err.network": "No se encuentra el servidor. Revisa que estés en la misma red Wi-Fi.",
  "pair.err.generic": "No se pudo emparejar.",
  "pair.err.unauthorized": "Este servidor pide emparejar el dispositivo.",
  "pair.device": "Nombre del dispositivo",
  // connection
  "conn.open": "Conectado",
  "conn.connecting": "Conectando…",
  "conn.reconnecting": "Reconectando…",
  "conn.closed": "Sin conexión",
  "conn.offlineBanner": "Sin conexión. Reintentando…",
  "conn.forget": "Olvidar este dispositivo",
  // roles
  "role.operator": "Operador",
  "role.pastor": "Pastor",
  "role.musician": "Músico",
  "role.stage": "Escenario",
  "role.congregation": "Congregación",
  // tabs
  "tab.slides": "Slides",
  "tab.service": "Culto",
  "tab.bible": "Biblia",
  "tab.messages": "Mensajes",
  "tab.sections": "Secciones del remoto",
  // operator
  "op.live": "EN VIVO",
  "op.black": "NEGRO",
  "op.clear": "LIMPIAR",
  "op.logo": "LOGO",
  "op.prev": "Slide anterior",
  "op.next": "Slide siguiente",
  "op.nothing": "Nada en pantalla",
  "op.noShow": "No hay un culto cargado en el equipo.",
  "op.goLive": "En vivo",
  "op.goPreview": "A preview",
  "op.next.label": "Siguiente",
  "op.serviceMode": "Modo culto activo",
  "op.timers": "Cronómetros",
  "op.start": "Iniciar",
  "op.pause": "Pausar",
  "op.reset": "Reiniciar",
  "op.overtime": "Pasado",
  "op.itemClock": "Tiempo del ítem",
  "op.takePreview": "Pasar preview a vivo",
  "msg.title": "Mensaje al escenario",
  "msg.placeholder": "Escribe un aviso para el equipo…",
  "msg.send": "Enviar",
  "msg.remove": "Quitar mensaje",
  "msg.current": "Mostrando",
  "msg.quick1": "Terminar ya",
  "msg.quick2": "Repetir coro",
  "msg.quick3": "Un minuto",
  "msg.quick4": "Sube el volumen",
  // pastor / bible
  "bible.title": "Biblia",
  "bible.sub.pastor": "Rol: Pastor · pide versículos al operador",
  "bible.sub.operator": "Busca y proyecta versículos",
  "bible.search": "Referencia o texto",
  "bible.searchHint": "Ej. «ro 8 28» o «amor perfecto»",
  "bible.reader": "vista lector",
  "bible.toPreview": "Enviar a preview",
  "bible.toLive": "Pedir en vivo",
  "bible.toLive.operator": "Proyectar en vivo",
  "bible.note": "El operador confirma «Pedir en vivo» según los permisos del rol.",
  "bible.select": "Toca un versículo para elegirlo; toca otro para extender el rango.",
  "bible.noVersions": "No hay versiones bíblicas instaladas.",
  "bible.noResults": "Sin resultados.",
  "bible.prev": "Capítulo anterior",
  "bible.next": "Capítulo siguiente",
  "bible.onScreen": "en pantalla",
  "bible.sent": "Enviado",
  "bible.layout": "Disposición",
  "bible.layout.columns": "Columnas",
  "bible.layout.stacked": "Apilado",
  "bible.layout.subtitle": "Subtítulo",
  "bible.loading": "Cargando…",
  // musician
  "mus.title": "Acordes",
  "mus.transpose": "Transponer",
  "mus.key": "semitonos",
  "mus.reset": "Original",
  "mus.next": "Sigue",
  "mus.noChords": "Esta diapositiva no tiene acordes.",
  "mus.list": "Orden del culto",
  // congregation
  "crowd.live": "En vivo · sigue el culto",
  "crowd.waiting": "El culto aún no ha empezado.",
  "crowd.smaller": "Reducir texto",
  "crowd.bigger": "Agrandar texto",
  "crowd.lang": "Idioma",
  "crowd.lang.both": "Español + English",
  "crowd.lang.es": "Español",
  "crowd.lang.en": "English",
  "crowd.version": "Versión bíblica",
  "crowd.versionAuto": "La del proyector",
  "crowd.theme": "Tema claro u oscuro",
  "crowd.prayer": "Enviar petición de oración",
  "crowd.prayer.title": "Petición de oración",
  "crowd.prayer.text": "¿Por qué podemos orar?",
  "crowd.prayer.name": "Tu nombre (opcional)",
  "crowd.prayer.send": "Enviar",
  "crowd.prayer.cancel": "Cancelar",
  "crowd.prayer.sent": "Gracias, recibimos tu petición.",
  "crowd.prayer.queued": "Guardada. Se enviará cuando haya conexión con el servidor.",
  "crowd.prayer.empty": "Escribe tu petición.",
  "crowd.blackout": "Pantalla en negro",
  // misc
  "common.close": "Cerrar",
  "common.back": "Volver",
  "common.error": "Algo salió mal",
  "err.version_mismatch": "",
  "err.forbidden": "Tu rol no permite esta acción.",
  "err.locked": "Modo culto: esta acción está bloqueada.",
  "err.generic": "No se pudo completar la acción.",
  "err.offline": "Sin conexión con el equipo.",
  "lang.switch": "English",
} as const;

export type MessageKey = keyof typeof es;

const en: Record<MessageKey, string> = {
  appName: "HollyShow Remote",
  "pair.title": "Connect your phone",
  "pair.hint": "Type the 6-digit PIN shown on the operator screen, or scan its QR code.",
  "pair.pin": "PIN",
  "pair.connect": "Connect",
  "pair.checking": "Checking…",
  "pair.follow": "Follow the service without a code",
  "pair.err.invalid": "Wrong or expired PIN.",
  "pair.err.locked": "Too many attempts. Wait a moment.",
  "pair.err.network": "Server not found. Make sure you are on the same Wi-Fi network.",
  "pair.err.generic": "Pairing failed.",
  "pair.err.unauthorized": "This server requires pairing this device.",
  "pair.device": "Device name",
  "conn.open": "Connected",
  "conn.connecting": "Connecting…",
  "conn.reconnecting": "Reconnecting…",
  "conn.closed": "Offline",
  "conn.offlineBanner": "Offline. Retrying…",
  "conn.forget": "Forget this device",
  "role.operator": "Operator",
  "role.pastor": "Pastor",
  "role.musician": "Musician",
  "role.stage": "Stage",
  "role.congregation": "Congregation",
  "tab.slides": "Slides",
  "tab.service": "Service",
  "tab.bible": "Bible",
  "tab.messages": "Messages",
  "tab.sections": "Remote sections",
  "op.live": "LIVE",
  "op.black": "BLACK",
  "op.clear": "CLEAR",
  "op.logo": "LOGO",
  "op.prev": "Previous slide",
  "op.next": "Next slide",
  "op.nothing": "Nothing on screen",
  "op.noShow": "No service is loaded on the host.",
  "op.goLive": "Live",
  "op.goPreview": "To preview",
  "op.next.label": "Next",
  "op.serviceMode": "Service mode on",
  "op.timers": "Timers",
  "op.start": "Start",
  "op.pause": "Pause",
  "op.reset": "Reset",
  "op.overtime": "Over",
  "op.itemClock": "Item time",
  "op.takePreview": "Take preview live",
  "msg.title": "Message to the stage",
  "msg.placeholder": "Type a note for the team…",
  "msg.send": "Send",
  "msg.remove": "Remove message",
  "msg.current": "Showing",
  "msg.quick1": "Wrap up",
  "msg.quick2": "Repeat chorus",
  "msg.quick3": "One minute",
  "msg.quick4": "Volume up",
  "bible.title": "Bible",
  "bible.sub.pastor": "Role: Pastor · requests verses from the operator",
  "bible.sub.operator": "Search and project verses",
  "bible.search": "Reference or text",
  "bible.searchHint": "E.g. “ro 8 28” or “perfect love”",
  "bible.reader": "reader view",
  "bible.toPreview": "Send to preview",
  "bible.toLive": "Request live",
  "bible.toLive.operator": "Project live",
  "bible.note": "The operator confirms “Request live” according to the role permissions.",
  "bible.select": "Tap a verse to pick it; tap another to extend the range.",
  "bible.noVersions": "No Bible versions installed.",
  "bible.noResults": "No results.",
  "bible.prev": "Previous chapter",
  "bible.next": "Next chapter",
  "bible.onScreen": "on screen",
  "bible.sent": "Sent",
  "bible.layout": "Layout",
  "bible.layout.columns": "Columns",
  "bible.layout.stacked": "Stacked",
  "bible.layout.subtitle": "Subtitle",
  "bible.loading": "Loading…",
  "mus.title": "Chords",
  "mus.transpose": "Transpose",
  "mus.key": "semitones",
  "mus.reset": "Original",
  "mus.next": "Next",
  "mus.noChords": "This slide has no chords.",
  "mus.list": "Service order",
  "crowd.live": "Live · follow the service",
  "crowd.waiting": "The service has not started yet.",
  "crowd.smaller": "Smaller text",
  "crowd.bigger": "Larger text",
  "crowd.lang": "Language",
  "crowd.lang.both": "Español + English",
  "crowd.lang.es": "Español",
  "crowd.lang.en": "English",
  "crowd.version": "Bible version",
  "crowd.versionAuto": "Same as projector",
  "crowd.theme": "Light or dark theme",
  "crowd.prayer": "Send a prayer request",
  "crowd.prayer.title": "Prayer request",
  "crowd.prayer.text": "What can we pray for?",
  "crowd.prayer.name": "Your name (optional)",
  "crowd.prayer.send": "Send",
  "crowd.prayer.cancel": "Cancel",
  "crowd.prayer.sent": "Thank you, we received your request.",
  "crowd.prayer.queued": "Saved. It will be sent when the server is reachable.",
  "crowd.prayer.empty": "Write your request.",
  "crowd.blackout": "Screen is black",
  "common.close": "Close",
  "common.back": "Back",
  "common.error": "Something went wrong",
  "err.version_mismatch": "",
  "err.forbidden": "Your role does not allow this action.",
  "err.locked": "Service mode: this action is locked.",
  "err.generic": "The action could not be completed.",
  "err.offline": "Not connected to the host.",
  "lang.switch": "Español",
};

const dicts: Record<Lang, Record<MessageKey, string>> = { es, en };

export function detectLang(): Lang {
  const saved = safeStorage.get(KEYS.lang);
  if (saved === "es" || saved === "en") return saved;
  const nav = typeof navigator === "undefined" ? "es" : navigator.language;
  return nav.toLowerCase().startsWith("en") ? "en" : "es";
}

let current: Lang = detectLang();
const listeners = new Set<() => void>();

export function getLang(): Lang {
  return current;
}

export function setLang(l: Lang): void {
  if (l === current) return;
  current = l;
  safeStorage.set(KEYS.lang, l);
  if (typeof document !== "undefined") document.documentElement.lang = l;
  listeners.forEach((f) => f());
}

export function translate(
  lang: Lang,
  key: MessageKey,
  vars?: Record<string, string | number>,
): string {
  let s = dicts[lang][key] ?? dicts.es[key] ?? key;
  if (vars) for (const [k, v] of Object.entries(vars)) s = s.replace(`{${k}}`, String(v));
  return s;
}

export function useT(): (key: MessageKey, vars?: Record<string, string | number>) => string {
  const lang = useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
  return (key, vars) => translate(lang, key, vars);
}

export function useLang(): Lang {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
}
