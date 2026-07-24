--
-- SQLite CRUD operations
--
-- Demonstrates db_exec and db_query nodes with a file-based SQLite database.
-- Each step connects to the same file, so the data persists between steps.
-- Effects:
-- - Creates and removes one UUID-scoped SQLite file under TMPDIR, TMP, TEMP,
--   or `.`. A failed run may leave that uniquely named database for inspection.
--

local flow = Flow.new("sqlite_crud")

local temp_root = env("TMPDIR")
if temp_root == nil or temp_root == "" then temp_root = env("TMP") end
if temp_root == nil or temp_root == "" then temp_root = env("TEMP") end
if temp_root == nil or temp_root == "" then temp_root = "." end
local database_path = temp_root .. "/ironflow-sqlite-" .. uuid4() .. ".db"
local db = "sqlite:" .. database_path .. "?mode=rwc"

-- Create table
flow:step("create_table", nodes.db_exec({
    connection = db,
    query = "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)"
}))

-- Insert rows
flow:step("insert_alice", nodes.db_exec({
    connection = db,
    query = "INSERT INTO users (name, email) VALUES (?, ?)",
    params = { "Alice", "alice@example.com" }
})):depends_on("create_table")

flow:step("insert_bob", nodes.db_exec({
    connection = db,
    query = "INSERT INTO users (name, email) VALUES (?, ?)",
    params = { "Bob", "bob@example.com" }
})):depends_on("create_table")

-- Query all rows
flow:step("query_all", nodes.db_query({
    connection = db,
    query = "SELECT * FROM users",
    output_key = "users"
})):depends_on("insert_alice"):depends_on("insert_bob")

flow:step("done", nodes.log({
    message = "Found ${ctx.users_count} users"
})):depends_on("query_all")

flow:step("cleanup", nodes.delete_file({
    path = database_path
})):depends_on("done")

return flow
