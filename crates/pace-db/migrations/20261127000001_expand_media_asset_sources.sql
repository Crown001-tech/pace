-- Allow stock-matched and AI-generated media asset sources in addition to upload/extension.
ALTER TABLE media_assets DROP CONSTRAINT IF EXISTS media_assets_source_check;
ALTER TABLE media_assets
    ADD CONSTRAINT media_assets_source_check
    CHECK (source IN ('upload', 'extension', 'stock', 'ai_gen'));
