-- Buscar actualizaciones al abrir la app: desactivado por defecto (local-first).
ALTER TABLE app_settings ADD COLUMN check_updates_on_start INTEGER NOT NULL DEFAULT 0 CHECK (check_updates_on_start IN (0, 1));
