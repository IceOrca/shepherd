local session_key = KEYS[1]
local user_sessions_key = KEYS[2]

local expected_account_id = ARGV[1]
local sid = ARGV[2]

if redis.call('EXISTS', session_key) == 0 then
    redis.call('ZREM', user_sessions_key, sid)
    return { 'none' }
end

local session_account_id = redis.call('HGET', session_key, 'account_id')
if tostring(session_account_id) ~= tostring(expected_account_id) then
    return { 'none' }
end

local jti = redis.call('HGET', session_key, 'jti')
local jti_exp = redis.call('HGET', session_key, 'jti_exp')
redis.call('DEL', session_key)
redis.call('ZREM', user_sessions_key, sid)
return { 'revoked', jti or '', jti_exp or '0' }
