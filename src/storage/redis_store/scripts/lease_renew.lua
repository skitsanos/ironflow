if #KEYS ~= 2 or #ARGV ~= 5 then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_ARGUMENTS')
end
local run_type = redis.call('TYPE', KEYS[1]).ok
if run_type == 'none' then return 0 end
if run_type ~= 'hash' then return redis.error_reply('IRONFLOW_INVALID_RUN_TYPE') end
if redis.call('HGET', KEYS[1], 'lease_owner') ~= ARGV[1] then return 0 end
local current_expiry = redis.call('HGET', KEYS[1], 'lease_expires_micros')
if current_expiry == false then return 0 end
if string.match(current_expiry, '^%-?%d+$') == nil or tonumber(current_expiry) == nil then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_EXPIRY')
end
local time = redis.call('TIME')
local now = (tonumber(time[1]) * 1000000) + tonumber(time[2])
if tonumber(current_expiry) <= now then return 0 end
local raw_info = redis.call('HGET', KEYS[1], 'info')
local info_ok, info = pcall(cjson.decode, raw_info)
if not info_ok or type(info) ~= 'table' or info['id'] ~= ARGV[2] then
    return redis.error_reply('IRONFLOW_CURRENT_RUN_OWNER_MISMATCH')
end
if info['status'] ~= 'pending' and info['status'] ~= 'running' then return 0 end
if string.match(ARGV[3], '^[1-9]%d*$') == nil or tonumber(ARGV[3]) == nil then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_TTL')
end
local ttl = ARGV[4]
if ttl ~= '-1' and (string.match(ttl, '^[1-9]%d*$') == nil or tonumber(ttl) == nil) then
    return redis.error_reply('IRONFLOW_INVALID_TTL')
end
if string.match(ARGV[5], '^%d+$') == nil or tonumber(ARGV[5]) == nil then
    return redis.error_reply('IRONFLOW_INVALID_RUN_LEASE_SAFETY')
end
local expiry = string.format('%.0f', now + tonumber(ARGV[3]))
redis.call('HSET', KEYS[1], 'lease_expires_micros', expiry)
redis.call('ZADD', KEYS[2], expiry, ARGV[2])
if ttl ~= '-1' then
    local active_ttl = math.ceil((tonumber(ARGV[3]) + tonumber(ARGV[5])) / 1000000)
    redis.call('EXPIRE', KEYS[1], math.max(active_ttl, tonumber(ttl)))
else redis.call('PERSIST', KEYS[1]) end
return 1
