local session_key = KEYS[1]
local user_sessions_key = KEYS[2]

local sid = ARGV[1]
local ttl_secs = tonumber(ARGV[2])
local max_session = tonumber(ARGV[3])
local score = tonumber(ARGV[4])
local session_key_prefix = ARGV[5]

if not max_session or max_session < 1 then
    return { 'invalid_limit' }
end

local revoked = { 'ok' }

local sids = redis.call('ZRANGE', user_sessions_key, 0, -1)
for _, old_sid in ipairs(sids) do
    local old_session_key = session_key_prefix .. old_sid
    if redis.call('EXISTS', old_session_key) == 0 then
        redis.call('ZREM', user_sessions_key, old_sid)
    end
end

-- Preserve creation order when multiple sessions are created in the same
-- second. This script is atomic, so advancing the latest score is stable.
local newest = redis.call('ZREVRANGE', user_sessions_key, 0, 0, 'WITHSCORES')
if newest[2] and tonumber(newest[2]) >= score then
    score = tonumber(newest[2]) + 1
end

-- Reserve a slot before adding the new SID.
while redis.call('ZCARD', user_sessions_key) >= max_session do
    local kicked_sid = redis.call('ZRANGE', user_sessions_key, 0, 0)[1]
    if not kicked_sid then
        break
    end

    local kicked_key = session_key_prefix .. kicked_sid
    local kicked_jti = redis.call('HGET', kicked_key, 'jti')
    local kicked_jti_expires_at = redis.call('HGET', kicked_key, 'jti_exp')
    redis.call('DEL', kicked_key)
    redis.call('ZREM', user_sessions_key, kicked_sid)
    table.insert(revoked, kicked_jti or '')
    table.insert(revoked, kicked_jti_expires_at or '0')
end

redis.call('DEL', session_key)
for i = 6, #ARGV, 2 do
    redis.call('HSET', session_key, ARGV[i], ARGV[i + 1])
end
redis.call('EXPIRE', session_key, ttl_secs)

redis.call('ZADD', user_sessions_key, score, sid)
redis.call('EXPIRE', user_sessions_key, ttl_secs)

return revoked
