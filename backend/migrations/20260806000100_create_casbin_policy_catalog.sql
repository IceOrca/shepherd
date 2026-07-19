-- Casbin's SQLx adapter performs compile-time query validation, so this
-- shared policy catalog must be created by project migrations before the
-- optional PostgreSQL adapter is compiled. Tenant domains are stored in
-- Casbin policy fields and enforced by each application's model.
CREATE TABLE casbin_rule (
    id SERIAL PRIMARY KEY,
    ptype VARCHAR NOT NULL,
    v0 VARCHAR NOT NULL,
    v1 VARCHAR NOT NULL,
    v2 VARCHAR NOT NULL,
    v3 VARCHAR NOT NULL,
    v4 VARCHAR NOT NULL,
    v5 VARCHAR NOT NULL,
    CONSTRAINT casbin_rule_policy_uq UNIQUE (ptype, v0, v1, v2, v3, v4, v5)
);

CREATE INDEX casbin_rule_ptype_v1_idx ON casbin_rule (ptype, v1);
CREATE INDEX casbin_rule_ptype_v2_idx ON casbin_rule (ptype, v2);
