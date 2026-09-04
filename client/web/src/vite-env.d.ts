/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_SHEPHERD_AUTH_URL?: string;
  readonly VITE_PLANNED_STAFFING_ENABLED?: string;
  readonly VITE_STAFFING_GPS_ENABLED?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
