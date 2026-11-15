-- Bounded replacements for the standard-library functions that can loop
-- without bound inside one C call (IF-213). Loaded once per sandboxed state
-- with the Rust pattern functions as its argument.
local rust = ...
local FAIL, RAW = rust.FAIL, rust.RAW
local find, match, gmatch, gsub = rust.find, rust.match, rust.gmatch, rust.gsub
local rep, insert, remove, move = string.rep, table.insert, table.remove, table.move
local native_gsub = string.gsub
local getmetatable, select, error, type, pcall = getmetatable, select, error, type, pcall
local tonumber, tointeger, maxinteger = tonumber, math.tointeger, math.maxinteger

-- A Rust function reports its own failure as (FAIL, message) and a failure
-- of a Lua callback as (RAW, message), so both are raised here as ordinary
-- string errors: the first at the caller's position, the second unchanged.
-- A caller that invokes a replacement in tail position has no frame left to
-- name, so its error carries no position.
local function finish(...)
  local first = ...
  if first == FAIL then error(select(2, ...), 2) end
  if first == RAW then error(select(2, ...), 0) end
  return ...
end

string.find = function(...) return finish(find(...)) end
string.match = function(...) return finish(match(...)) end
string.gsub = function(...) return finish(gsub(...)) end
string.gmatch = function(...)
  local iterate, message = gmatch(...)
  if iterate == FAIL then error(message, 2) end
  return function() return finish(iterate()) end
end

-- A reference function called through `pcall` reports neither its name nor
-- the caller's position; restore both.
local function relay(name, ok, ...)
  if ok then return ... end
  local message = ...
  if type(message) ~= "string" then error(message, 0) end
  error(native_gsub(message, "to '%?'", "to '" .. name .. "'", 1), 2)
end

-- The reference `string.rep` loops `n` times to build an empty result.
string.rep = function(...)
  local s, n, sep = ...
  if s == "" and (sep == nil or sep == "") then
    local count = tonumber(n)
    if count ~= nil and tointeger(count) ~= nil then return "" end
  end
  return relay("rep", pcall(rep, ...))
end

-- `table.insert` and `table.remove` shift up to `#t` elements, and `__len`
-- may claim any length; `table.move` accepts any index range. Past their
-- fast paths these run as Lua loops, which the instruction hook interrupts.
local NATIVE_MOVE_LIMIT = 100000

local function plain(t)
  return type(t) ~= "table" or getmetatable(t) == nil
end

local function length(t)
  local n = tointeger(#t)
  if n == nil then error("object length is not an integer", 3) end
  return n
end

local function position(value, name)
  local pos = tointeger(tonumber(value))
  if pos == nil then
    error("bad argument #2 to '" .. name .. "' (number expected, got " .. type(value) .. ")", 3)
  end
  return pos
end

table.insert = function(...)
  local t = ...
  if plain(t) then return relay("insert", pcall(insert, ...)) end
  local count = select("#", ...) - 1
  local e = length(t) + 1
  if count == 1 then
    t[e] = select(2, ...)
  elseif count == 2 then
    local _, pos, value = ...
    pos = position(pos, "insert")
    if pos < 1 or pos > e then
      error("bad argument #2 to 'insert' (position out of bounds)", 2)
    end
    for i = e, pos + 1, -1 do t[i] = t[i - 1] end
    t[pos] = value
  else
    error("wrong number of arguments to 'insert'", 2)
  end
end

table.remove = function(...)
  local t, pos = ...
  if plain(t) then return relay("remove", pcall(remove, ...)) end
  local size = length(t)
  if pos == nil then
    pos = size
  else
    pos = position(pos, "remove")
    if pos ~= size and (pos < 1 or pos - 1 > size) then
      error("bad argument #2 to 'remove' (position out of bounds)", 2)
    end
  end
  local value = t[pos]
  for i = pos, size - 1 do t[i] = t[i + 1] end
  if pos < size then pos = size end
  t[pos] = nil
  return value
end

table.move = function(...)
  local a1, f, e, t, a2 = ...
  local first, last, target = tointeger(f), tointeger(e), tointeger(t)
  if first == nil or last == nil or target == nil or last < first
    or (first <= 0 and last >= maxinteger + first)
    or last - first < NATIVE_MOVE_LIMIT then
    return relay("move", pcall(move, ...))
  end
  local count = last - first + 1
  if target > maxinteger - count + 1 then
    error("bad argument #4 to 'move' (destination wrap around)", 2)
  end
  local destination = a2
  if destination == nil then destination = a1 end
  if target > last or target <= first or a1 ~= destination then
    for i = 0, count - 1 do destination[target + i] = a1[first + i] end
  else
    for i = count - 1, 0, -1 do destination[target + i] = a1[first + i] end
  end
  return destination
end
