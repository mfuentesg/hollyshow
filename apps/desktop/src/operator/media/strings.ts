import { useTranslation } from "react-i18next";

/**
 * Strings of the image bank panel. Kept next to the component (the shared locale files belong to
 * other owners); the active language comes from the app's i18next instance. Spanish is the default.
 */
const es = {
  title: "Buscar fondos",
  subtitle: "Imágenes con licencia abierta de Openverse y Wikimedia Commons",
  searchLabel: "Buscar imágenes",
  searchPlaceholder: "Ej.: cruz, amanecer, adoración…",
  search: "Buscar",
  categories: "Categorías",
  source: "Fuente",
  sourceAll: "Todas las fuentes",
  sourceOpenverse: "Openverse",
  sourceCommons: "Wikimedia Commons",
  noncommercial: "Incluir licencias no comerciales (NC/ND)",
  tabSearch: "Buscar",
  tabLibrary: "Descargadas",
  translated: "Buscando: {{q}}",
  loading: "Buscando imágenes…",
  results: "Resultados de imágenes",
  empty: "Sin resultados. Prueba con otra palabra o categoría.",
  hint: "Elige una categoría o escribe lo que buscas. Las imágenes se guardan en tu equipo: no se necesita internet durante el culto.",
  loadMore: "Cargar más",
  unavailable: "El banco de imágenes no está disponible en esta versión.",
  failure: "{{source}}: {{message}}",
  rateLimited: "{{source}} pidió esperar. Reintenta en {{s}} s.",
  libraryEmpty: "Aún no has descargado imágenes.",
  library: "Imágenes descargadas",
  download: "Descargar",
  downloading: "Descargando…",
  downloaded: "Descargada",
  useAsBackground: "Usar como fondo",
  remove: "Eliminar",
  close: "Cerrar",
  open: "Ver {{title}}",
  preview: "Vista previa",
  author: "Autor",
  unknownAuthor: "Autor desconocido",
  license: "Licencia",
  origin: "Fuente",
  size: "Tamaño",
  attribution: "Atribución",
  copyAttribution: "Copiar atribución",
  copied: "Copiado",
  exportCredits: "Exportar créditos",
  creditsExported: "Créditos exportados",
  downloadFailed: "No se pudo descargar: {{message}}",
  downloadedToast: "Imagen guardada en la biblioteca",
  licenseOk: "Uso libre",
  licenseAttrib: "Requiere atribución",
  licenseNc: "Restringida",
};

const en: typeof es = {
  title: "Find backgrounds",
  subtitle: "Openly licensed images from Openverse and Wikimedia Commons",
  searchLabel: "Search images",
  searchPlaceholder: "E.g. cross, sunrise, worship…",
  search: "Search",
  categories: "Categories",
  source: "Source",
  sourceAll: "All sources",
  sourceOpenverse: "Openverse",
  sourceCommons: "Wikimedia Commons",
  noncommercial: "Include non-commercial licenses (NC/ND)",
  tabSearch: "Search",
  tabLibrary: "Downloaded",
  translated: "Searching: {{q}}",
  loading: "Searching images…",
  results: "Image results",
  empty: "No results. Try another word or category.",
  hint: "Pick a category or type what you need. Images are saved on this computer: no internet is needed during the service.",
  loadMore: "Load more",
  unavailable: "The image bank is not available in this version.",
  failure: "{{source}}: {{message}}",
  rateLimited: "{{source}} asked us to wait. Retry in {{s}} s.",
  libraryEmpty: "You have not downloaded any images yet.",
  library: "Downloaded images",
  download: "Download",
  downloading: "Downloading…",
  downloaded: "Downloaded",
  useAsBackground: "Use as background",
  remove: "Remove",
  close: "Close",
  open: "View {{title}}",
  preview: "Preview",
  author: "Author",
  unknownAuthor: "Unknown author",
  license: "License",
  origin: "Source",
  size: "Size",
  attribution: "Attribution",
  copyAttribution: "Copy attribution",
  copied: "Copied",
  exportCredits: "Export credits",
  creditsExported: "Credits exported",
  downloadFailed: "Could not download: {{message}}",
  downloadedToast: "Image saved to the library",
  licenseOk: "Free use",
  licenseAttrib: "Attribution required",
  licenseNc: "Restricted",
};

export type MediaKey = keyof typeof es;
export type MediaT = (key: MediaKey, vars?: Record<string, string | number>) => string;

const dictionaries = { es, en } as const;

export function useMediaT(): { t: MediaT; lang: "es" | "en" } {
  const { i18n } = useTranslation();
  const lang = (i18n.language ?? "es").startsWith("en") ? "en" : "es";
  const dict = dictionaries[lang];
  const t: MediaT = (key, vars) =>
    dict[key].replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(vars?.[name] ?? ""));
  return { t, lang };
}
