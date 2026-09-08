-- Platform authority is independent of tenant-owned roles and permissions.
CREATE TABLE platform_administrators (
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL CHECK (length(subject) BETWEEN 1 AND 255 AND subject = btrim(subject)),
    username TEXT NOT NULL,
    email TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (issuer, subject)
);

CREATE TABLE platform_administration_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_issuer TEXT NOT NULL,
    actor_subject TEXT NOT NULL,
    action TEXT NOT NULL,
    details JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (actor_issuer, actor_subject) REFERENCES platform_administrators (issuer, subject)
);

CREATE FUNCTION shepherd_platform_audit_immutable() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Platform administration events are immutable' USING ERRCODE = '23514';
END;
$$;
CREATE TRIGGER platform_administration_events_immutable
BEFORE UPDATE OR DELETE ON platform_administration_events
FOR EACH ROW EXECUTE FUNCTION shepherd_platform_audit_immutable();

-- These platform catalogs deliberately have no tenant context. HTTP access is
-- restricted to verified identities in platform_administrators, never JWT roles.
