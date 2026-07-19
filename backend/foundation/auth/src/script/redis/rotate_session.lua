local session_key = KEYS[1]

local sid = ARGV[1]
local presented_rti = ARGV[2]
local new_rti = ARGV[3]
local new_jti = ARGV[4]
local new_jti_expires_at = ARGV[5]
local ttl_secs = tonumber(ARGV[6])
local now = ARGV[7]
local refresh_expires_at = ARGV[8]
local fallback_idle_timeout_secs = tonumber(ARGV[9])
local user_order_key_prefix = ARGV[10]
local new_role = ARGV[11]
local new_idle_timeout_secs = ARGV[12]
local new_auth_version = ARGV[13]

if redis.call('EXISTS', session_key) == 0 then
    return { 'not_found' }
end

local stored_sid = redis.call('HGET', session_key, 'sid')
local account_id = redis.call('HGET', session_key, 'account_id')
local old_jti = redis.call('HGET', session_key, 'jti')
local old_jti_expires_at = redis.call('HGET', session_key, 'jti_exp')
local stored_refresh_key = redis.call('HGET', session_key, 'rti')
local old_expires_at = tonumber(redis.call('HGET', session_key, 'expires_at') or '0')
local last_rotate = tonumber(redis.call('HGET', session_key, 'last_rotate') or '0')
-- Rotation is called only after current-account authorization succeeds, so
-- role changes must use the current policy immediately.
local idle_timeout_secs = tonumber(new_idle_timeout_secs or tostring(fallback_idle_timeout_secs))

if not account_id or account_id == '' then
    redis.call('DEL', session_key)
    return { 'mismatch', old_jti or '', old_jti_expires_at or '0' }
end

local user_sessions_key = user_order_key_prefix .. account_id

if stored_sid ~= sid then
    redis.call('DEL', session_key)
    redis.call('ZREM', user_sessions_key, sid)
    return { 'mismatch', old_jti or '', old_jti_expires_at or '0' }
end

if tonumber(now) >= old_expires_at then
    redis.call('DEL', session_key)
    redis.call('ZREM', user_sessions_key, sid)
    return { 'expired', old_jti or '', old_jti_expires_at or '0' }
end

if idle_timeout_secs > 0 and tonumber(now) >= (last_rotate + idle_timeout_secs) then
    redis.call('DEL', session_key)
    redis.call('ZREM', user_sessions_key, sid)
    return { 'idle_timeout', old_jti or '', old_jti_expires_at or '0' }
end

if stored_refresh_key ~= presented_rti then
    redis.call('DEL', session_key)
    redis.call('ZREM', user_sessions_key, sid)
    return { 'mismatch', old_jti or '', old_jti_expires_at or '0' }
end

redis.call('HSET', session_key, 'rti', new_rti)
redis.call('HSET', session_key, 'role', new_role)
redis.call('HSET', session_key, 'auth_version', new_auth_version)
redis.call('HSET', session_key, 'jti', new_jti)
redis.call('HSET', session_key, 'jti_exp', new_jti_expires_at)
redis.call('HSET', session_key, 'last_rotate', now)
redis.call('HSET', session_key, 'idle_timeout_secs', new_idle_timeout_secs)
redis.call('HSET', session_key, 'expires_at', refresh_expires_at)
redis.call('EXPIRE', session_key, ttl_secs)
-- The ordered set score records session creation order for limit eviction.
-- Refresh activity is tracked by last_rotate and must not reorder sessions.
redis.call('EXPIRE', user_sessions_key, ttl_secs)

local result = { 'ok', old_jti or '', old_jti_expires_at or '0' }
local rotated_session = redis.call('HGETALL', session_key)
for _, field in ipairs(rotated_session) do
    table.insert(result, field)
end
return result
