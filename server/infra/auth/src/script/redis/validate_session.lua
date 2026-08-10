local session_key = KEYS[1]

local sid = ARGV[1]
local presented_rti = ARGV[2]
local now = tonumber(ARGV[3])
local fallback_idle_timeout_secs = tonumber(ARGV[4])
local user_order_key_prefix = ARGV[5]

if redis.call('EXISTS', session_key) == 0 then
    return { 'not_found' }
end

local stored_sid = redis.call('HGET', session_key, 'sid')
local account_id = redis.call('HGET', session_key, 'account_id')
local old_jti = redis.call('HGET', session_key, 'jti')
local old_jti_expires_at = redis.call('HGET', session_key, 'jti_exp')
local stored_refresh_key = redis.call('HGET', session_key, 'rti')
local expires_at = tonumber(redis.call('HGET', session_key, 'expires_at') or '0')
local last_rotate = tonumber(redis.call('HGET', session_key, 'last_rotate') or '0')
local idle_timeout_secs = tonumber(redis.call('HGET', session_key, 'idle_timeout_secs') or tostring(fallback_idle_timeout_secs))

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

if now >= expires_at then
    redis.call('DEL', session_key)
    redis.call('ZREM', user_sessions_key, sid)
    return { 'expired', old_jti or '', old_jti_expires_at or '0' }
end

if idle_timeout_secs > 0 and now >= (last_rotate + idle_timeout_secs) then
    redis.call('DEL', session_key)
    redis.call('ZREM', user_sessions_key, sid)
    return { 'idle_timeout', old_jti or '', old_jti_expires_at or '0' }
end

if stored_refresh_key ~= presented_rti then
    redis.call('DEL', session_key)
    redis.call('ZREM', user_sessions_key, sid)
    return { 'mismatch', old_jti or '', old_jti_expires_at or '0' }
end

local result = { 'ok' }
local session = redis.call('HGETALL', session_key)
for _, field in ipairs(session) do
    table.insert(result, field)
end
return result
