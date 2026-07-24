if #KEYS ~= 15 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_KEYS')
end
if #ARGV ~= 6 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_ARGUMENTS')
end

local owner = ARGV[1]
local token = ARGV[2]
local generation = ARGV[3]
local expected_phase = ARGV[4]
local expected_cursor = ARGV[5]
local expected_digest = ARGV[6]
if ironflow_key_type(KEYS[9]) == 'none' then
    return {'stale'}
end
local progress = ironflow_load_progress(owner, token)
if progress == nil
    or not ironflow_matches(
        progress, token, generation, expected_phase, expected_cursor, expected_digest) then
    return {'stale'}
end
ironflow_assert_key_layout(progress.mode)
if progress.phase == 'blocked' then
    return {'blocked'}
end

local function set_progress(phase, generation_value, cursor, digest, expected)
    progress.phase = phase
    progress.generation = generation_value
    progress.generation_text = tostring(generation_value)
    progress.cursor = cursor
    progress.cursor_text = tostring(cursor)
    progress.digest = digest
    progress.expected_digest = expected
    return ironflow_progress_response(progress)
end

local function mark_blocked()
    redis.call('HSET', KEYS[9],
        'phase', 'blocked',
        'generation', tostring(progress.generation + 1))
    return {'blocked'}
end

local function raw_window_digest(start, count, max_bytes)
    local digest = redis.sha1hex('ironflow-legacy-restore-batch-v2')
    local total_bytes = 0
    for item = 0, count - 1 do
        local raw = redis.call('LINDEX', KEYS[10], start + item)
        if raw == false then
            return nil
        end
        local raw_bytes = string.len(raw)
        if raw_bytes > max_bytes
            or (item > 0 and total_bytes + raw_bytes > max_bytes) then
            return nil
        end
        total_bytes = total_bytes + raw_bytes
        digest = redis.sha1hex(digest .. redis.sha1hex(
            tostring(item) .. ':' .. raw_bytes .. ':' .. raw))
    end
    return digest
end

local function restore_tail_window(progress)
    local maximum_count = math.min(progress.batch, progress.cursor)
    local reversed = {}
    local total_bytes = 0
    for distance = 1, maximum_count do
        local raw = redis.call('LINDEX', KEYS[10], -distance)
        if raw == false then
            return nil
        end
        local raw_bytes = string.len(raw)
        if raw_bytes > progress.max_bytes then
            return nil
        end
        if #reversed > 0 and total_bytes + raw_bytes > progress.max_bytes then
            break
        end
        reversed[#reversed + 1] = raw
        total_bytes = total_bytes + raw_bytes
    end
    if #reversed == 0 then
        return nil
    end

    local digest = redis.sha1hex('ironflow-legacy-restore-batch-v2')
    local item = 0
    for position = #reversed, 1, -1 do
        local raw = reversed[position]
        digest = redis.sha1hex(digest .. redis.sha1hex(
            tostring(item) .. ':' .. string.len(raw) .. ':' .. raw))
        item = item + 1
    end
    return {count = #reversed, digest = digest}
end

local function restore_family()
    if not ironflow_sources_absent(progress) then
        return mark_blocked()
    end
    local target_offset = progress.mode == 'raw' and 4 or 0
    local presence = progress.presence
    if type(presence) ~= 'string' or string.len(presence) ~= 4 then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_PRESENCE')
    end
    ironflow_preflight_finalize()
    for position = 1, 4 do
        local expected = string.sub(presence, position, position) == '1'
        local snapshot_type = ironflow_key_type(KEYS[9 + position])
        local target_type = ironflow_key_type(KEYS[target_offset + position])
        if expected then
            if snapshot_type == 'none' or target_type ~= 'none' then
                return mark_blocked()
            end
        elseif snapshot_type ~= 'none' or target_type ~= 'none' then
            return mark_blocked()
        end
    end
    local failure_code = progress.failure_code
    if type(failure_code) ~= 'string' or failure_code == '' then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_FAILURE')
    end
    local minimum_ttl, ttl_is_live = ironflow_snapshot_minimum_ttl(progress)
    if not ttl_is_live then
        ironflow_expire_snapshot_soon()
        return {'expiring'}
    end
    ironflow_align_snapshot_ttl(minimum_ttl)
    for position = 1, 4 do
        if string.sub(presence, position, position) == '1' then
            redis.call('RENAME', KEYS[9 + position], KEYS[target_offset + position])
        end
    end
    redis.call('DEL', KEYS[9])
    return {'failed', failure_code}
end

local function finalize_family()
    local snapshot_exists = ironflow_family_presence(9)
    local current_exists = ironflow_family_presence(0)
    if snapshot_exists == 0 then
        if current_exists == 0 then
            return mark_blocked()
        end
        local current = ironflow_family(0)
        if current.layout ~= '2' or current.owner ~= owner then
            return mark_blocked()
        end
        redis.call('DEL', KEYS[9])
        return {'current'}
    end
    if current_exists ~= 0
        or (progress.mode == 'raw' and not ironflow_family_absent(4)) then
        return mark_blocked()
    end
    local snapshot, status = ironflow_snapshot(progress)
    if snapshot == nil then
        return status == 'blocked' and mark_blocked() or {'blocked'}
    end

    ironflow_preflight_finalize()
    local minimum_ttl, ttl_is_live = ironflow_snapshot_minimum_ttl(progress)
    if not ttl_is_live then
        ironflow_expire_snapshot_soon()
        return {'expiring'}
    end

    if progress.phase ~= 'finalizing' then
        local next_generation = progress.generation + 1
        redis.call('HSET', KEYS[9],
            'phase', 'finalizing',
            'generation', tostring(next_generation))
        progress.phase = 'finalizing'
        progress.generation = next_generation
        progress.generation_text = tostring(next_generation)
    end
    redis.call('HSET', KEYS[13], 'layout', '2', 'run_id', owner)
    ironflow_align_snapshot_ttl(minimum_ttl)
    -- Data components move before metadata. Until the final metadata rename,
    -- current writers reject the incomplete unmarked family.
    for position = 1, 3 do
        if ironflow_key_type(KEYS[9 + position]) ~= 'none' then
            redis.call('RENAME', KEYS[9 + position], KEYS[position])
        end
    end
    redis.call('RENAME', KEYS[13], KEYS[4])
    redis.call('DEL', KEYS[9])
    return {'current'}
end

local function restart_scan()
    local restart_count = ironflow_parse_uint(
        redis.call('HGET', KEYS[9], 'restart_count') or '0',
        'IRONFLOW_INVALID_EVENT_MIGRATION_RESTART') + 1
    if restart_count > 32 then
        return mark_blocked()
    end
    local seed = ironflow_digest_seed(owner, progress.sequence_text)
    local next_generation = progress.generation + 1
    redis.call('HSET', KEYS[9],
        'phase', 'scan',
        'generation', tostring(next_generation),
        'cursor', '0',
        'digest', seed,
        'restart_count', tostring(restart_count))
    redis.call('HDEL', KEYS[9],
        'expected_digest', 'pending_count', 'pending_digest',
        'pending_next_cursor', 'pending_next_digest')
    return set_progress('scan', next_generation, 0, seed, false)
end

if progress.phase == 'scan_pending' or progress.phase == 'verify_pending' then
    if not ironflow_sources_absent(progress) then
        return mark_blocked()
    end
    local pending_count_text = redis.call('HGET', KEYS[9], 'pending_count')
    local pending_count = ironflow_parse_uint(
        pending_count_text, 'IRONFLOW_INVALID_EVENT_MIGRATION_PENDING')
    local pending_digest = redis.call('HGET', KEYS[9], 'pending_digest')
    local pending_next_cursor = redis.call('HGET', KEYS[9], 'pending_next_cursor')
    local pending_next_digest = redis.call('HGET', KEYS[9], 'pending_next_digest')
    ironflow_validate_digest(pending_digest)
    ironflow_validate_digest(pending_next_digest)
    local pending_next = ironflow_parse_uint(
        pending_next_cursor, 'IRONFLOW_INVALID_EVENT_MIGRATION_PENDING')
    if pending_count < 1 or pending_count > progress.batch
        or pending_next ~= progress.cursor + pending_count then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_PENDING')
    end
    local batch, status = ironflow_read_batch(progress, owner, true, pending_count)
    if batch == nil or batch.count ~= pending_count or batch.digest ~= pending_digest then
        if status == 'expired' then
            return mark_blocked()
        end
        return mark_blocked()
    end
    local base_phase = ironflow_phase_base(progress.phase)
    if base_phase == 'verify' and pending_next == progress.sequence then
        progress.cursor = pending_next
        progress.cursor_text = pending_next_cursor
        progress.digest = pending_next_digest
        if pending_next_digest ~= progress.expected_digest then
            return restart_scan()
        end
        return finalize_family()
    end
    local next_generation = progress.generation + 1
    redis.call('HSET', KEYS[9],
        'phase', base_phase,
        'generation', tostring(next_generation),
        'cursor', pending_next_cursor,
        'digest', pending_next_digest)
    redis.call('HDEL', KEYS[9],
        'pending_count', 'pending_digest',
        'pending_next_cursor', 'pending_next_digest')
    return set_progress(
        base_phase, next_generation, tonumber(pending_next_cursor),
        pending_next_digest, progress.expected_digest)
end

if progress.phase == 'restore_pending' then
    local pending_count = ironflow_parse_uint(
        redis.call('HGET', KEYS[9], 'pending_count'),
        'IRONFLOW_INVALID_EVENT_MIGRATION_PENDING')
    local pending_digest = redis.call('HGET', KEYS[9], 'pending_digest')
    local pending_next = ironflow_parse_uint(
        redis.call('HGET', KEYS[9], 'pending_next_cursor'),
        'IRONFLOW_INVALID_EVENT_MIGRATION_PENDING')
    local head_digest = raw_window_digest(0, pending_count, progress.max_bytes)
    if head_digest == nil or head_digest ~= pending_digest
        or pending_next + pending_count ~= progress.cursor then
        return mark_blocked()
    end
    local next_generation = progress.generation + 1
    redis.call('HSET', KEYS[9],
        'phase', 'restore',
        'generation', tostring(next_generation),
        'cursor', tostring(pending_next))
    redis.call('HDEL', KEYS[9],
        'pending_count', 'pending_digest', 'pending_next_cursor')
    return set_progress(
        'restore', next_generation, pending_next,
        progress.digest, progress.expected_digest)
end

if progress.phase == 'restore' then
    if progress.cursor == 0 then
        return restore_family()
    end
    local snapshot, status = ironflow_snapshot(progress)
    if snapshot == nil or not ironflow_sources_absent(progress) then
        return status == 'blocked' and mark_blocked() or mark_blocked()
    end
    local window = restore_tail_window(progress)
    if window == nil then
        return mark_blocked()
    end
    local count = window.count
    local pending_digest = window.digest
    ironflow_preflight_lmove()
    local next_cursor = progress.cursor - count
    local next_generation = progress.generation + 1
    redis.call('HSET', KEYS[9],
        'phase', 'restore_pending',
        'generation', tostring(next_generation),
        'pending_count', tostring(count),
        'pending_digest', pending_digest,
        'pending_next_cursor', tostring(next_cursor))
    for _ = 1, count do
        redis.call('LMOVE', KEYS[10], KEYS[10], 'RIGHT', 'LEFT')
    end
    progress.phase = 'restore_pending'
    progress.generation = next_generation
    progress.generation_text = tostring(next_generation)
    return ironflow_progress_response(progress)
end

if progress.phase == 'finalizing' then
    return finalize_family()
end
if progress.phase ~= 'scan' and progress.phase ~= 'verify' then
    return {'stale'}
end
if progress.cursor ~= progress.sequence then
    return redis.error_reply('IRONFLOW_EVENT_MIGRATION_PHASE_INCOMPLETE')
end
if not ironflow_sources_absent(progress) then
    return mark_blocked()
end

if progress.phase == 'scan' then
    local seed = ironflow_digest_seed(owner, progress.sequence_text)
    local next_generation = progress.generation + 1
    redis.call('HSET', KEYS[9],
        'phase', 'verify',
        'generation', tostring(next_generation),
        'cursor', '0',
        'digest', seed,
        'expected_digest', progress.digest)
    return set_progress('verify', next_generation, 0, seed, progress.digest)
end

if progress.digest ~= progress.expected_digest or progress.sequence > 0 then
    return restart_scan()
end
return finalize_family()
