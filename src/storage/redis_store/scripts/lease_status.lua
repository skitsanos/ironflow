if #KEYS ~= 10 or #ARGV ~= 14 then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_ARGUMENTS')
end
if ARGV[6] ~= '-1' and
   (string.match(ARGV[6], '^[1-9]%d*$') == nil or tonumber(ARGV[6]) == nil) then
    return redis.error_reply('IRONFLOW_INVALID_TTL')
end
if ARGV[13] ~= '0' and ARGV[13] ~= '1' then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_RELEASE')
end
if string.match(ARGV[14], '^%d+$') == nil or tonumber(ARGV[14]) == nil then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_SAFETY')
end
local run_type = redis.call('TYPE', KEYS[1]).ok
if run_type == 'none' then redis.call('ZREM', KEYS[10], ARGV[8]); return -3 end
if run_type ~= 'hash' then return redis.error_reply('IRONFLOW_INVALID_RUN_TYPE') end
if redis.call('HEXISTS', KEYS[1], 'info') == 0 then
    return redis.error_reply('IRONFLOW_RUN_INFO_MISSING')
end

local owner = redis.call('HGET', KEYS[1], 'lease_owner')
local expiry = redis.call('HGET', KEYS[1], 'lease_expires_micros')
if owner == false or expiry == false then redis.call('ZREM', KEYS[10], ARGV[8]); return -2 end
if string.match(expiry, '^%-?%d+$') == nil or tonumber(expiry) == nil then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_EXPIRY')
end
local time = redis.call('TIME')
local now = (tonumber(time[1]) * 1000000) + tonumber(time[2])
if ARGV[11] == 'owner' then
    if owner ~= ARGV[12] or tonumber(expiry) <= now then return -2 end
elseif ARGV[11] == 'expired' then
    if string.match(expiry, '^%-?%d+$') == nil or tonumber(expiry) == nil then
        return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_EXPIRY')
    end
    if tonumber(expiry) > now then return -2 end
else
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_GUARD')
end

local status_keys = {
    pending = 4, running = 5, success = 6, failed = 7, stalled = 8, cancelled = 9
}
local status_key = status_keys[ARGV[10]]
if status_key == nil then return redis.error_reply('IRONFLOW_INVALID_ORDERED_STATUS') end
local incarnation = redis.call('HGET', KEYS[1], 'incarnation')
if incarnation == false then incarnation = ARGV[3] end
if incarnation ~= ARGV[3] then return -1 end
local revision = redis.call('HGET', KEYS[1], 'revision')
if revision == false then revision = ARGV[1] end
if revision ~= ARGV[2] then return 0 end

redis.call('HSET', KEYS[1], 'info', ARGV[4], 'summary', ARGV[5],
    'revision', ARGV[7], 'incarnation', ARGV[3])
local old_member = redis.call('HGET', KEYS[2], ARGV[8])
if old_member ~= false then
    for index = 3, 9 do redis.call('ZREM', KEYS[index], old_member) end
end
redis.call('HSET', KEYS[2], ARGV[8], ARGV[9])
redis.call('ZADD', KEYS[3], 0, ARGV[9])
redis.call('ZADD', KEYS[status_key], 0, ARGV[9])
if ARGV[13] == '1' then
    redis.call('HDEL', KEYS[1], 'lease_owner', 'lease_expires_micros')
    redis.call('ZREM', KEYS[10], ARGV[8])
end
if ARGV[13] == '1' then
    if ARGV[6] ~= '-1' then redis.call('EXPIRE', KEYS[1], ARGV[6])
    else redis.call('PERSIST', KEYS[1]) end
else
    if ARGV[6] ~= '-1' then
        local active_ttl = math.ceil((tonumber(expiry) - now + tonumber(ARGV[14])) / 1000000)
        redis.call('EXPIRE', KEYS[1], math.max(active_ttl, tonumber(ARGV[6]), 1))
    else redis.call('PERSIST', KEYS[1]) end
end
return 1
