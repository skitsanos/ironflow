local IRONFLOW_MAX_SAFE_INTEGER = 9007199254740990
local IRONFLOW_MAX_BATCH_SIZE = 128
local IRONFLOW_MAX_BATCH_BYTES = 1048576

local function ironflow_fail(marker)
    error(marker, 0)
end

local function ironflow_parse_uint(raw, marker)
    if type(raw) ~= 'string'
        or (raw ~= '0' and string.match(raw, '^[1-9][0-9]*$') == nil) then
        ironflow_fail(marker)
    end
    local value = tonumber(raw)
    if value == nil or value < 0 or value ~= math.floor(value)
        or value > IRONFLOW_MAX_SAFE_INTEGER then
        ironflow_fail(marker)
    end
    return value
end

local function ironflow_key_type(key)
    return redis.call('TYPE', key).ok
end

local function ironflow_require_type(key, expected, marker)
    local actual = ironflow_key_type(key)
    if actual ~= 'none' and actual ~= expected then
        ironflow_fail(marker)
    end
    return actual
end

local function ironflow_family_presence(offset)
    local count = 0
    for position = 1, 4 do
        if ironflow_key_type(KEYS[offset + position]) ~= 'none' then
            count = count + 1
        end
    end
    return count
end

local function ironflow_family_absent(offset)
    return ironflow_family_presence(offset) == 0
end

local function ironflow_family(offset)
    local list_type = ironflow_require_type(
        KEYS[offset + 1], 'list', 'IRONFLOW_INVALID_EVENT_LIST_TYPE')
    local index_type = ironflow_require_type(
        KEYS[offset + 2], 'hash', 'IRONFLOW_INVALID_EVENT_INDEX_TYPE')
    local sequence_type = ironflow_require_type(
        KEYS[offset + 3], 'string', 'IRONFLOW_INVALID_EVENT_SEQUENCE_TYPE')
    local meta_type = ironflow_require_type(
        KEYS[offset + 4], 'hash', 'IRONFLOW_INVALID_EVENT_META_TYPE')

    local presence = ''
    local exists = 0
    for _, key_type in ipairs({list_type, index_type, sequence_type, meta_type}) do
        if key_type == 'none' then
            presence = presence .. '0'
        else
            presence = presence .. '1'
            exists = exists + 1
        end
    end

    local list_len = redis.call('LLEN', KEYS[offset + 1])
    local index_len = redis.call('HLEN', KEYS[offset + 2])
    local sequence_text = redis.call('GET', KEYS[offset + 3]) or '0'
    local sequence = ironflow_parse_uint(
        sequence_text, 'IRONFLOW_INVALID_EVENT_SEQUENCE')
    if sequence ~= list_len or sequence ~= index_len then
        ironflow_fail('IRONFLOW_INCONSISTENT_EVENT_STORE')
    end

    return {
        exists = exists,
        presence = presence,
        sequence = sequence,
        sequence_text = sequence_text,
        layout = redis.call('HGET', KEYS[offset + 4], 'layout'),
        owner = redis.call('HGET', KEYS[offset + 4], 'run_id')
    }
end

local function ironflow_validate_layout(family, expected_owner)
    if family.layout ~= false and family.layout ~= '2' then
        ironflow_fail('IRONFLOW_UNSUPPORTED_EVENT_LAYOUT')
    end
    if family.owner ~= false and family.owner ~= expected_owner then
        ironflow_fail('IRONFLOW_EVENT_LAYOUT_OWNER_MISMATCH')
    end
end

local function ironflow_validate_policy(batch_text, byte_text)
    local batch = ironflow_parse_uint(batch_text, 'IRONFLOW_INVALID_EVENT_MIGRATION_BATCH')
    local max_bytes = ironflow_parse_uint(
        byte_text, 'IRONFLOW_INVALID_EVENT_MIGRATION_BYTES')
    if batch < 1 or batch > IRONFLOW_MAX_BATCH_SIZE then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_BATCH')
    end
    if max_bytes < 1 or max_bytes > IRONFLOW_MAX_BATCH_BYTES then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_BYTES')
    end
    return batch, max_bytes
end

local function ironflow_validate_token(token)
    if type(token) ~= 'string' or string.len(token) ~= 32
        or string.match(token, '^[0-9a-f]+$') == nil then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_TOKEN')
    end
end

local function ironflow_validate_digest(digest)
    if type(digest) ~= 'string' or string.len(digest) ~= 40
        or string.match(digest, '^[0-9a-f]+$') == nil then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_DIGEST')
    end
end

local function ironflow_digest_seed(owner, sequence_text)
    return redis.sha1hex(
        'ironflow-legacy-events-v2:' .. string.len(owner) .. ':' .. owner .. ':' .. sequence_text)
end

local function ironflow_now_ms()
    local now = redis.call('TIME')
    return tonumber(now[1]) * 1000 + math.floor(tonumber(now[2]) / 1000)
end

local function ironflow_expiry_deadline(offset)
    local minimum_ttl = nil
    for position = 1, 4 do
        if ironflow_key_type(KEYS[offset + position]) ~= 'none' then
            local ttl = redis.call('PTTL', KEYS[offset + position])
            if ttl >= 0 and (minimum_ttl == nil or ttl < minimum_ttl) then
                minimum_ttl = ttl
            end
        end
    end
    if minimum_ttl == nil then
        return '-1'
    end
    return string.format('%.0f', ironflow_now_ms() + minimum_ttl)
end

local function ironflow_snapshot_minimum_ttl(progress)
    local minimum_ttl = nil
    if progress.expiry_at >= 0 then
        local captured_remaining = progress.expiry_at - ironflow_now_ms()
        if captured_remaining <= 0 then
            return nil, false
        end
        minimum_ttl = captured_remaining
    end
    for position = 1, 4 do
        if ironflow_key_type(KEYS[9 + position]) ~= 'none' then
            local ttl = redis.call('PTTL', KEYS[9 + position])
            if ttl >= 0 and (minimum_ttl == nil or ttl < minimum_ttl) then
                minimum_ttl = ttl
            end
        end
    end
    if minimum_ttl ~= nil and minimum_ttl <= 0 then
        return nil, false
    end
    return minimum_ttl, true
end

local function ironflow_align_snapshot_ttl(minimum_ttl)
    for position = 1, 4 do
        if ironflow_key_type(KEYS[9 + position]) ~= 'none' then
            if minimum_ttl == nil then
                redis.call('PERSIST', KEYS[9 + position])
            else
                redis.call(
                    'PEXPIRE', KEYS[9 + position], string.format('%.0f', minimum_ttl))
            end
        end
    end
end

local function ironflow_expire_snapshot_soon()
    for position = 1, 4 do
        if ironflow_key_type(KEYS[9 + position]) ~= 'none' then
            redis.call('PEXPIRE', KEYS[9 + position], 1)
        end
    end
end

local function ironflow_digest_step(digest, position, raw, event_id, index_text)
    local framed = tostring(position) .. ':'
        .. string.len(raw) .. ':' .. raw .. ':'
        .. string.len(event_id) .. ':' .. event_id .. ':'
        .. string.len(index_text) .. ':' .. index_text
    return redis.sha1hex(digest .. redis.sha1hex(framed))
end

local function ironflow_batch_seed()
    return redis.sha1hex('ironflow-legacy-event-batch-v2')
end

local function ironflow_phase_base(phase)
    if phase == 'scan' or phase == 'scan_pending' then
        return 'scan'
    end
    if phase == 'verify' or phase == 'verify_pending' then
        return 'verify'
    end
    return nil
end

local function ironflow_load_progress(expected_owner, expected_token)
    if ironflow_key_type(KEYS[9]) ~= 'hash' then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_STATE')
    end
    if redis.call('HGET', KEYS[9], 'version') ~= '2'
        or redis.call('HGET', KEYS[9], 'run_id') ~= expected_owner then
        ironflow_fail('IRONFLOW_EVENT_MIGRATION_OWNER_MISMATCH')
    end

    local token = redis.call('HGET', KEYS[9], 'token')
    ironflow_validate_token(token)
    if expected_token ~= nil and token ~= expected_token then
        return nil
    end
    local phase = redis.call('HGET', KEYS[9], 'phase')
    local valid_phases = {
        freezing = true, scan = true, scan_pending = true,
        verify = true, verify_pending = true, restore = true,
        restore_pending = true, finalizing = true, blocked = true
    }
    if valid_phases[phase] ~= true then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_PHASE')
    end
    local mode = redis.call('HGET', KEYS[9], 'mode')
    if mode ~= 'current' and mode ~= 'raw' then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_MODE')
    end
    local generation_text = redis.call('HGET', KEYS[9], 'generation')
    local sequence_text = redis.call('HGET', KEYS[9], 'sequence')
    local cursor_text = redis.call('HGET', KEYS[9], 'cursor')
    local generation = ironflow_parse_uint(
        generation_text, 'IRONFLOW_INVALID_EVENT_MIGRATION_GENERATION')
    local sequence = ironflow_parse_uint(
        sequence_text, 'IRONFLOW_INVALID_EVENT_MIGRATION_SEQUENCE')
    local cursor = ironflow_parse_uint(
        cursor_text, 'IRONFLOW_INVALID_EVENT_MIGRATION_CURSOR')
    if cursor > sequence then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_CURSOR')
    end
    local batch_text = redis.call('HGET', KEYS[9], 'batch')
    local max_bytes_text = redis.call('HGET', KEYS[9], 'max_bytes')
    local batch, max_bytes = ironflow_validate_policy(batch_text, max_bytes_text)
    local digest = redis.call('HGET', KEYS[9], 'digest')
    ironflow_validate_digest(digest)
    local expected_digest = redis.call('HGET', KEYS[9], 'expected_digest')
    if expected_digest ~= false then
        ironflow_validate_digest(expected_digest)
    end
    local expiry_at_text = redis.call('HGET', KEYS[9], 'expiry_at_ms')
    local expiry_at = -1
    if expiry_at_text ~= '-1' then
        expiry_at = ironflow_parse_uint(
            expiry_at_text, 'IRONFLOW_INVALID_EVENT_MIGRATION_EXPIRY')
    end

    return {
        token = token, phase = phase, mode = mode,
        generation = generation, generation_text = generation_text,
        sequence = sequence, sequence_text = sequence_text,
        cursor = cursor, cursor_text = cursor_text,
        batch = batch, batch_text = batch_text,
        max_bytes = max_bytes, max_bytes_text = max_bytes_text,
        digest = digest, expected_digest = expected_digest,
        presence = redis.call('HGET', KEYS[9], 'presence'),
        expiry_at = expiry_at, expiry_at_text = expiry_at_text,
        failure_code = redis.call('HGET', KEYS[9], 'failure_code')
    }
end

local function ironflow_progress_response(progress)
    return {
        'progress', progress.phase, progress.token, progress.mode,
        progress.generation_text, progress.cursor_text, progress.sequence_text,
        progress.batch_text, progress.max_bytes_text, progress.digest,
        progress.expected_digest or '-'
    }
end

local function ironflow_matches(progress, token, generation, phase, cursor, digest)
    return progress.token == token
        and progress.generation_text == generation
        and progress.phase == phase
        and progress.cursor_text == cursor
        and progress.digest == digest
end

local function ironflow_snapshot(progress)
    local snapshot = ironflow_family(9)
    if snapshot.exists == 0 then
        return nil, 'expired'
    end
    if snapshot.sequence_text ~= progress.sequence_text then
        return nil, 'blocked'
    end
    ironflow_validate_layout(snapshot, redis.call('HGET', KEYS[9], 'run_id'))
    return snapshot, nil
end

local function ironflow_sources_absent(progress)
    if not ironflow_family_absent(0) then
        return false
    end
    if progress.mode == 'raw' and not ironflow_family_absent(4) then
        return false
    end
    return true
end

local function ironflow_read_batch(progress, owner, from_tail, forced_count)
    local snapshot, status = ironflow_snapshot(progress)
    if snapshot == nil then
        return nil, status
    end
    local count = progress.sequence - progress.cursor
    if forced_count ~= nil then
        if forced_count < 1 or forced_count > progress.batch or forced_count > count then
            ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_PENDING')
        end
        count = forced_count
    elseif count > progress.batch then
        count = progress.batch
    end
    if count == 0 then
        return {payloads = {}, count = 0, digest = ironflow_batch_seed()}, nil
    end

    local payloads = {}
    local ids = {}
    local indexes = {}
    local total_bytes = 0
    local digest = ironflow_batch_seed()
    for item = 0, count - 1 do
        local list_position = item
        if from_tail then
            list_position = item - count
        end
        local raw = redis.call('LINDEX', KEYS[10], list_position)
        if raw == false then
            return nil, 'inconsistent_list'
        end
        local raw_bytes = string.len(raw)
        if raw_bytes > progress.max_bytes then
            return nil, 'oversized'
        end
        if #payloads > 0 and total_bytes + raw_bytes > progress.max_bytes then
            break
        end
        local ok, event = pcall(cjson.decode, raw)
        if not ok or type(event) ~= 'table' or type(event['id']) ~= 'string'
            or event['id'] == '' then
            return nil, 'invalid_payload'
        end
        if event['run_id'] ~= owner then
            return nil, 'run_mismatch'
        end
        local logical_position = progress.cursor + #payloads
        local index_text = redis.call('HGET', KEYS[11], event['id'])
        if index_text ~= tostring(logical_position + 1) then
            return nil, 'index_mismatch'
        end
        payloads[#payloads + 1] = raw
        ids[#ids + 1] = event['id']
        indexes[#indexes + 1] = index_text
        total_bytes = total_bytes + raw_bytes
        digest = ironflow_digest_step(
            digest, logical_position, raw, event['id'], index_text)
    end
    return {
        payloads = payloads, ids = ids, indexes = indexes,
        count = #payloads, digest = digest,
        next_cursor = progress.cursor + #payloads
    }, nil
end

local function ironflow_batch_rolling_digest(progress, batch)
    local digest = progress.digest
    for item = 1, batch.count do
        digest = ironflow_digest_step(
            digest, progress.cursor + item - 1, batch.payloads[item],
            batch.ids[item], batch.indexes[item])
    end
    return digest
end

local function ironflow_assert_key_layout(mode)
    local positions = {1, 2, 3, 4, 9, 10, 11, 12, 13, 14, 15}
    if mode == 'raw' then
        positions[#positions + 1] = 5
        positions[#positions + 1] = 6
        positions[#positions + 1] = 7
        positions[#positions + 1] = 8
    end
    for left = 1, #positions do
        for right = left + 1, #positions do
            if KEYS[positions[left]] == KEYS[positions[right]] then
                ironflow_fail('IRONFLOW_EVENT_KEYS_MUST_BE_DISTINCT')
            end
        end
    end
end

local function ironflow_require_empty_probes()
    if ironflow_key_type(KEYS[14]) ~= 'none'
        or ironflow_key_type(KEYS[15]) ~= 'none' then
        ironflow_fail('IRONFLOW_EVENT_MIGRATION_PROBE_COLLISION')
    end
end

local function ironflow_preflight_lmove()
    ironflow_require_empty_probes()
    redis.call('LMOVE', KEYS[14], KEYS[14], 'LEFT', 'RIGHT')
end

local function ironflow_preflight_finalize()
    ironflow_require_empty_probes()
    redis.call('PERSIST', KEYS[14])
    redis.call('PEXPIRE', KEYS[14], 1)
    redis.call('DEL', KEYS[14], KEYS[15])
    local result = redis.pcall('RENAME', KEYS[14], KEYS[15])
    if type(result) ~= 'table' or result.err == nil then
        ironflow_fail('IRONFLOW_EVENT_MIGRATION_RENAME_PREFLIGHT_FAILED')
    end
    if string.find(string.lower(result.err), 'no such key', 1, true) == nil then
        error(result.err, 0)
    end
end
