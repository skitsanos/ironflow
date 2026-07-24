local current_type = redis.call('TYPE', KEYS[1]).ok
if current_type ~= 'none' then
    if current_type ~= 'hash' then
        return redis.error_reply('IRONFLOW_INVALID_CURRENT_RUN_TYPE')
    end
    local current_raw_info = redis.call('HGET', KEYS[1], 'info')
    local current_info_ok, current_info = pcall(cjson.decode, current_raw_info)
    if not current_info_ok or type(current_info) ~= 'table' or current_info['id'] ~= ARGV[1] then
        return redis.error_reply('IRONFLOW_CURRENT_RUN_OWNER_MISMATCH')
    end
    return 0
end

local legacy_type = redis.call('TYPE', KEYS[2]).ok
if legacy_type == 'none' then
    return 0
end

-- A historical run named `index` aliases the old global index key. A Set is
-- therefore the catalog, not a migratable run record.
if KEYS[2] == KEYS[3] and legacy_type == 'set' then
    return 0
end
if legacy_type ~= 'hash' or redis.call('HEXISTS', KEYS[2], 'info') == 0 then
    if KEYS[2] ~= KEYS[3] and redis.call('SISMEMBER', KEYS[3], ARGV[1]) == 0 then
        return 0
    end
    return redis.error_reply('IRONFLOW_INVALID_LEGACY_RUN')
end
local raw_info = redis.call('HGET', KEYS[2], 'info')
local info_ok, info = pcall(cjson.decode, raw_info)
if not info_ok or type(info) ~= 'table' or info['id'] ~= ARGV[1] then
    if KEYS[2] ~= KEYS[3] and redis.call('SISMEMBER', KEYS[3], ARGV[1]) == 0 then
        return 0
    end
    return redis.error_reply('IRONFLOW_LEGACY_RUN_ID_MISMATCH')
end

if KEYS[2] ~= KEYS[3] then
    local index_type = redis.call('TYPE', KEYS[3]).ok
    if index_type ~= 'none' and index_type ~= 'set' then
        return redis.error_reply('IRONFLOW_INVALID_RUN_INDEX_TYPE')
    end
end

redis.call('RENAME', KEYS[2], KEYS[1])
redis.call('SADD', KEYS[3], ARGV[1])
return 1
