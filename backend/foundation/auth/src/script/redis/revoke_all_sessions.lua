local user_sessions_key = KEYS[1]

local session_key_prefix = ARGV[1]

local sids = redis.call('ZRANGE', user_sessions_key, 0, -1)
local revoked = {}
for _, sid in ipairs(sids) do
    local session_key = session_key_prefix .. sid
    local jti = redis.call('HGET', session_key, 'jti')
    local jti_exp = redis.call('HGET', session_key, 'jti_exp')
    if jti then
        table.insert(revoked, jti)
        table.insert(revoked, jti_exp or '0')
    end
    redis.call('DEL', session_key)
    redis.log(redis.LOG_NOTICE, "DEL key '" .. session_key .. "'")
end
redis.call('DEL', user_sessions_key)
return revoked
