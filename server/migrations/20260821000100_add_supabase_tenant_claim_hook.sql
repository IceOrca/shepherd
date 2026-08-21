-- Supabase Auth and Shepherd share one PostgreSQL database while retaining
-- separate auth and public schema ownership. GoTrue invokes this hook before
-- signing each access token.
CREATE OR REPLACE FUNCTION public.shepherd_custom_access_token_hook(event JSONB)
RETURNS JSONB
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = ''
AS $$
DECLARE
    claims JSONB := COALESCE(event -> 'claims', '{}'::JSONB);
    mapped_tenant_id UUID;
BEGIN
    SELECT identity.tenant_id
    INTO mapped_tenant_id
    FROM public.account_identities AS identity
    INNER JOIN public.tenants AS tenant
        ON tenant.id = identity.tenant_id
       AND tenant.status = 'active'
    WHERE identity.issuer = claims ->> 'iss'
      AND identity.subject = event ->> 'user_id'
    LIMIT 1;

    IF mapped_tenant_id IS NULL THEN
        -- An authenticated provider identity is not an authorized Shepherd
        -- account until the application mapping exists.
        claims := claims - 'tid';
    ELSE
        claims := jsonb_set(claims, '{tid}', to_jsonb(mapped_tenant_id::TEXT), TRUE);
    END IF;

    RETURN jsonb_set(event, '{claims}', claims, TRUE);
END;
$$;

REVOKE ALL
ON FUNCTION public.shepherd_custom_access_token_hook(JSONB)
FROM PUBLIC;

GRANT USAGE ON SCHEMA public TO supabase_auth_admin;

GRANT EXECUTE
ON FUNCTION public.shepherd_custom_access_token_hook(JSONB)
TO supabase_auth_admin;
