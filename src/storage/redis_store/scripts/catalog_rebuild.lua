if #KEYS ~= 12 or #ARGV ~= 4 then
    return redis.error_reply('IRONFLOW_INVALID_CATALOG_REBUILD_ARITY')
end
for left = 1, 12 do
    for right = left + 1, 12 do
        if KEYS[left] == KEYS[right] then
            return redis.error_reply('IRONFLOW_CATALOG_KEYS_MUST_BE_DISTINCT')
        end
    end
end

local action = ARGV[1]
local owner = ARGV[2]
local lease = ARGV[3]
if owner == '' then
    return redis.error_reply('IRONFLOW_INVALID_CATALOG_REBUILD_OWNER')
end
if string.match(lease, '^[1-9][0-9]*$') == nil or string.len(lease) > 6 then
    return redis.error_reply('IRONFLOW_INVALID_CATALOG_REBUILD_LEASE')
end

local lock_type = redis.call('TYPE', KEYS[1]).ok
if lock_type ~= 'none' and lock_type ~= 'string' then
    return redis.error_reply('IRONFLOW_INVALID_CATALOG_REBUILD_LOCK_TYPE')
end

if action == 'acquire' then
    if lock_type == 'string' then
        if redis.call('PTTL', KEYS[1]) < 0 then
            return redis.error_reply('IRONFLOW_INVALID_CATALOG_REBUILD_LOCK_TTL')
        end
        return 0
    end
    local acquired = redis.call('SET', KEYS[1], owner, 'NX', 'PX', lease)
    if acquired == false then
        return 0
    end
    return 1
end

if lock_type == 'none' or redis.call('GET', KEYS[1]) ~= owner then
    return 0
end

if action == 'renew' then
    redis.call('PEXPIRE', KEYS[1], lease)
    return 1
end
if action == 'reset' then
    redis.call(
        'DEL', KEYS[2], KEYS[3], KEYS[4], KEYS[5], KEYS[6], KEYS[7],
        KEYS[8], KEYS[9], KEYS[10], KEYS[11], KEYS[12]
    )
    redis.call('PEXPIRE', KEYS[1], lease)
    return 1
end
if action == 'finalize' then
    if string.len(ARGV[4]) ~= 32 or string.match(ARGV[4], '^[0-9a-f]+$') == nil then
        return redis.error_reply('IRONFLOW_INVALID_CATALOG_GENERATION')
    end
    redis.call('SET', KEYS[2], ARGV[4])
    redis.call('DEL', KEYS[1])
    return 1
end
if action == 'release' then
    redis.call('DEL', KEYS[1])
    return 1
end
return redis.error_reply('IRONFLOW_INVALID_CATALOG_REBUILD_ACTION')
