if #KEYS ~= 2 or #ARGV ~= 5 then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_ARGUMENTS')
end
local max_integer = 9007199254740991
local lease_ttl = tonumber(ARGV[3])
if string.match(ARGV[3], '^[1-9]%d*$') == nil or lease_ttl == nil or lease_ttl > max_integer then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_TTL')
end
local ttl = tonumber(ARGV[4])
if ARGV[4] ~= '-1' and
   (string.match(ARGV[4], '^[1-9]%d*$') == nil or ttl == nil or ttl > 99999999999) then
    return redis.error_reply('IRONFLOW_INVALID_TTL')
end
local safety = tonumber(ARGV[5])
if string.match(ARGV[5], '^%d+$') == nil or safety == nil or safety > max_integer then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_SAFETY')
end
if KEYS[1] == KEYS[2] then return redis.error_reply('IRONFLOW_STATE_KEY_ALIAS') end
local index_type = redis.call('TYPE', KEYS[2]).ok
if index_type ~= 'none' and index_type ~= 'zset' then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_KEY_TYPE')
end
local run_type = redis.call('TYPE', KEYS[1]).ok
if run_type == 'none' then return 0 end
if run_type ~= 'hash' then return redis.error_reply('IRONFLOW_INVALID_RUN_TYPE') end
if redis.call('HGET', KEYS[1], 'lease_owner') ~= ARGV[1] then return 0 end
local current_expiry = redis.call('HGET', KEYS[1], 'lease_expires_micros')
if current_expiry == false then return 0 end
local current_expiry_number = tonumber(current_expiry)
if string.match(current_expiry, '^%-?%d+$') == nil or current_expiry_number == nil or
   math.abs(current_expiry_number) > max_integer then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_EXPIRY')
end
local time = redis.call('TIME')
local now = (tonumber(time[1]) * 1000000) + tonumber(time[2])
if current_expiry_number <= now then return 0 end
local raw_info = redis.call('HGET', KEYS[1], 'info')
local info_ok, info = pcall(cjson.decode, raw_info)
if not info_ok or type(info) ~= 'table' or info['id'] ~= ARGV[2] then
    return redis.error_reply('IRONFLOW_CURRENT_RUN_OWNER_MISMATCH')
end
if info['status'] ~= 'pending' and info['status'] ~= 'running' then return 0 end
-- Validate the resulting deadline and TTL before changing either lease key.
local expiry_number = now + lease_ttl
if expiry_number > max_integer or lease_ttl + safety > max_integer then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_TTL')
end
local applied_ttl = math.max(math.ceil((lease_ttl + safety) / 1000000), ttl)
if applied_ttl > 99999999999 then return redis.error_reply('IRONFLOW_INVALID_TTL') end
local expiry = string.format('%.0f', expiry_number)
redis.call('HSET', KEYS[1], 'lease_expires_micros', expiry)
redis.call('ZADD', KEYS[2], expiry, ARGV[2])
if ttl ~= -1 then
    redis.call('EXPIRE', KEYS[1], string.format('%.0f', applied_ttl))
else redis.call('PERSIST', KEYS[1]) end
return 1
