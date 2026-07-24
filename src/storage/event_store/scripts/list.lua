local limit = tonumber(ARGV[2])
if limit == nil or limit < 0 or limit > 9007199254740990 or limit ~= math.floor(limit) then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_LIMIT')
end
for left = 1, 4 do
    for right = left + 1, 4 do
        if KEYS[left] == KEYS[right] then
            return redis.error_reply('IRONFLOW_EVENT_KEYS_MUST_BE_DISTINCT')
        end
    end
end
local list_type = redis.call('TYPE', KEYS[1]).ok
local index_type = redis.call('TYPE', KEYS[2]).ok
local seq_type = redis.call('TYPE', KEYS[3]).ok
local meta_type = redis.call('TYPE', KEYS[4]).ok
if list_type ~= 'none' and list_type ~= 'list' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_LIST_TYPE')
end
if index_type ~= 'none' and index_type ~= 'hash' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_INDEX_TYPE')
end
if seq_type ~= 'none' and seq_type ~= 'string' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_SEQUENCE_TYPE')
end
if meta_type ~= 'none' and meta_type ~= 'hash' then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_META_TYPE')
end

local list_len = redis.call('LLEN', KEYS[1])
local index_len = redis.call('HLEN', KEYS[2])
local raw_seq = redis.call('GET', KEYS[3])
local seq = 0
if raw_seq ~= false then
    if raw_seq ~= '0' and string.match(raw_seq, '^[1-9][0-9]*$') == nil then
        return redis.error_reply('IRONFLOW_INVALID_EVENT_SEQUENCE')
    end
    seq = tonumber(raw_seq)
    if seq == nil or seq < 0 or seq ~= math.floor(seq) or seq > 9007199254740990 then
        return redis.error_reply('IRONFLOW_INVALID_EVENT_SEQUENCE')
    end
end
if seq ~= list_len or seq ~= index_len then
    return redis.error_reply('IRONFLOW_INCONSISTENT_EVENT_STORE')
end
local layout = redis.call('HGET', KEYS[4], 'layout')
if layout ~= false and layout ~= '2' then
    return redis.error_reply('IRONFLOW_UNSUPPORTED_EVENT_LAYOUT')
end
if layout == false and list_len > 0 then
    return redis.error_reply('IRONFLOW_UNVALIDATED_LEGACY_EVENT_LAYOUT')
end
if layout == '2' and redis.call('HGET', KEYS[4], 'run_id') ~= ARGV[3] then
    return redis.error_reply('IRONFLOW_EVENT_LAYOUT_OWNER_MISMATCH')
end
if list_len == 0 then
    if ARGV[1] ~= '' then
        return redis.error_reply('IRONFLOW_EVENT_CURSOR_NOT_FOUND')
    end
    return {}
end

local start = 0
if ARGV[1] ~= '' then
    local cursor = redis.call('HGET', KEYS[2], ARGV[1])
    if cursor == false then
        return redis.error_reply('IRONFLOW_EVENT_CURSOR_NOT_FOUND')
    end
    local position = tonumber(cursor)
    if position == nil or position < 1 or position > list_len or position ~= math.floor(position) then
        return redis.error_reply('IRONFLOW_INVALID_EVENT_CURSOR')
    end
    local raw = redis.call('LINDEX', KEYS[1], position - 1)
    local item_ok, item = pcall(cjson.decode, raw)
    if not item_ok or item['id'] ~= ARGV[1] or item['run_id'] ~= ARGV[3] then
        return redis.error_reply('IRONFLOW_INCONSISTENT_EVENT_CURSOR')
    end
    start = position
end
if start >= list_len then
    return {}
end
if limit == 0 then
    return {}
end
local count = math.min(limit, list_len - start)
return redis.call('LRANGE', KEYS[1], start, start + count - 1)
