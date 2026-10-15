local script_dir = arg[0]:match("^(.*[/\\])") or ""
local json = dofile(script_dir .. "json.lua")
local mode = arg[1]
local data = json.decode(io.read("*a"))

if mode == "token" then
  local token = data.token
  if token == nil or token == false or token == "" or token == 0 or
      (type(token) == "table" and next(token) == nil) then
    token = ""
  end
  print(token)
  return
end

if mode ~= "bottle-url" then
  error("expected mode 'bottle-url' or 'token'")
end

local bottle = data.bottle or {}
local stable = bottle.stable or {}
local files = stable.files or {}
local keys = json.object_keys(files)
local machine = jit.arch:lower()
local arm = machine:find("arm", 1, true) ~= nil or machine == "aarch64"
local preferred = {}

for _, key in ipairs(keys) do
  if jit.os == "OSX" then
    if arm and key:sub(1, 6) == "arm64_" then
      preferred[#preferred + 1] = key
    elseif not arm and key:sub(1, 6) ~= "arm64_" and not key:find("linux", 1, true) then
      preferred[#preferred + 1] = key
    end
  elseif arm and key:find("arm64_linux", 1, true) then
    preferred[#preferred + 1] = key
  elseif not arm and key:find("x86_64_linux", 1, true) then
    preferred[#preferred + 1] = key
  end
end

for _, key in ipairs(keys) do
  if key == "all" then
    preferred[#preferred + 1] = key
    break
  end
end

for _, key in ipairs(keys) do
  preferred[#preferred + 1] = key
end

for _, key in ipairs(preferred) do
  local entry = files[key] or {}
  local url = entry.url
  if url and url ~= "" then
    print(url)
    break
  end
end
