if #KEYS ~= 15 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_KEYS')
end
if #ARGV ~= 5 then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_ARGUMENTS')
end

local owner = ARGV[1]
local proposed_token = ARGV[2]
local unsafe = ARGV[3]
local batch_text = ARGV[4]
local max_bytes_text = ARGV[5]
if owner == '' or (unsafe ~= '0' and unsafe ~= '1') then
    return redis.error_reply('IRONFLOW_INVALID_EVENT_MIGRATION_ARGUMENTS')
end
ironflow_validate_token(proposed_token)
local batch, max_bytes = ironflow_validate_policy(batch_text, max_bytes_text)

local function source_offset(mode)
    if mode == 'raw' then
        return 4
    end
    return 0
end

local function move_family_to_snapshot(progress)
    local offset = source_offset(progress.mode)
    local presence = progress.presence
    if type(presence) ~= 'string' or string.len(presence) ~= 4
        or string.match(presence, '^[01]+$') == nil then
        ironflow_fail('IRONFLOW_INVALID_EVENT_MIGRATION_PRESENCE')
    end
    ironflow_preflight_finalize()
    for position = 1, 4 do
        local source_type = ironflow_key_type(KEYS[offset + position])
        local snapshot_type = ironflow_key_type(KEYS[9 + position])
        local expected = string.sub(presence, position, position) == '1'
        if expected then
            if source_type ~= 'none' and snapshot_type == 'none' then
                redis.call('RENAME', KEYS[offset + position], KEYS[9 + position])
            elseif source_type ~= 'none' or snapshot_type == 'none' then
                ironflow_fail('IRONFLOW_EVENT_MIGRATION_FREEZE_COLLISION')
            end
        elseif source_type ~= 'none' or snapshot_type ~= 'none' then
            ironflow_fail('IRONFLOW_EVENT_MIGRATION_FREEZE_COLLISION')
        end
    end
    local minimum_ttl, ttl_is_live = ironflow_snapshot_minimum_ttl(progress)
    if ttl_is_live then
        ironflow_align_snapshot_ttl(minimum_ttl)
    else
        ironflow_expire_snapshot_soon()
    end
    local generation = progress.generation + 1
    redis.call('HSET', KEYS[9],
        'phase', 'scan',
        'generation', tostring(generation))
    progress.phase = 'scan'
    progress.generation = generation
    progress.generation_text = tostring(generation)
    return progress
end

local state_type = ironflow_key_type(KEYS[9])
if state_type ~= 'none' then
    local progress = ironflow_load_progress(owner, nil)
    ironflow_assert_key_layout(progress.mode)
    if progress.expiry_at >= 0 and progress.expiry_at <= ironflow_now_ms()
        and ironflow_family_absent(0)
        and ironflow_family_absent(9)
        and (progress.mode ~= 'raw' or ironflow_family_absent(4)) then
        redis.call('DEL', KEYS[9])
        return {'empty'}
    end
    if progress.phase == 'blocked' then
        return {'blocked'}
    end
    if progress.phase == 'freezing' then
        progress = move_family_to_snapshot(progress)
    end
    return ironflow_progress_response(progress)
end

-- A deterministic snapshot without its state is never interpreted as an
-- empty stream. Its rotation cursor is unknowable, so fail closed and retain
-- every component for explicit recovery.
if not ironflow_family_absent(9) then
    return {'orphaned'}
end

local mode = 'current'
local source = nil
if unsafe == '1' then
    local current_presence = ironflow_family_presence(0)
    if current_presence > 0 then
        local meta_type = ironflow_key_type(KEYS[4])
        local layout = false
        local marked_owner = false
        if meta_type == 'hash' then
            layout = redis.call('HGET', KEYS[4], 'layout')
            marked_owner = redis.call('HGET', KEYS[4], 'run_id')
        end
        if marked_owner == owner then
            source = ironflow_family(0)
            ironflow_validate_layout(source, owner)
            if layout == '2' then
                return {'current'}
            end
        else
            -- Encoded unsafe IDs can collide with historical raw IDs. Without an
            -- exact owner marker, even an empty family is ambiguous.
            return {'manual'}
        end
    end

    if source == nil then
        local raw_presence = ironflow_family_presence(4)
        if raw_presence == 0 then
            return {'empty'}
        end
        local raw_meta_type = ironflow_key_type(KEYS[8])
        if raw_meta_type ~= 'hash'
            or redis.call('HGET', KEYS[8], 'run_id') ~= owner then
            -- A raw candidate is optional. If it is not explicitly owned by this
            -- run, leave it untouched and allow the injective encoded namespace
            -- to behave as empty.
            return {'empty'}
        end
        local raw_layout = redis.call('HGET', KEYS[8], 'layout')
        if raw_layout ~= false and raw_layout ~= '2' then
            return redis.error_reply('IRONFLOW_UNSUPPORTED_EVENT_LAYOUT')
        end
        mode = 'raw'
        source = ironflow_family(4)
        ironflow_validate_layout(source, owner)
    end
else
    source = ironflow_family(0)
    ironflow_validate_layout(source, owner)
    if source.layout == '2' and source.owner == owner then
        return {'current'}
    end
    if source.exists == 0 then
        return {'empty'}
    end
end

ironflow_assert_key_layout(mode)
local digest = ironflow_digest_seed(owner, source.sequence_text)
local offset = source_offset(mode)
local expiry_at = ironflow_expiry_deadline(offset)
redis.call('HSET', KEYS[9],
    'version', '2',
    'run_id', owner,
    'token', proposed_token,
    'mode', mode,
    'phase', 'freezing',
    'generation', '0',
    'sequence', source.sequence_text,
    'cursor', '0',
    'batch', tostring(batch),
    'max_bytes', tostring(max_bytes),
    'digest', digest,
    'presence', source.presence,
    'expiry_at_ms', expiry_at,
    'restart_count', '0')

local progress = {
    token = proposed_token, mode = mode, phase = 'freezing',
    generation = 0, generation_text = '0',
    sequence = source.sequence, sequence_text = source.sequence_text,
    cursor = 0, cursor_text = '0',
    batch = batch, batch_text = tostring(batch),
    max_bytes = max_bytes, max_bytes_text = tostring(max_bytes),
    digest = digest, expected_digest = false,
    presence = source.presence,
    expiry_at = expiry_at == '-1' and -1 or tonumber(expiry_at),
    expiry_at_text = expiry_at
}
progress = move_family_to_snapshot(progress)
return ironflow_progress_response(progress)
