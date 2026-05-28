ALTER TABLE integrations ALTER COLUMN url DROP NOT NULL;

UPDATE integrations SET url = NULL WHERE kind = 'email';
