-- Lua 5.0 semantics probe for the 5.0 chunk loader (Native patch 4 of rilua).
-- semantics.luac: Lua 5.0.3 luac -o semantics.luac semantics.lua (64-bit build)
-- semantics.expected: Lua 5.0.3 lua semantics.luac
local W = {}
local function w(...) for i = 1, arg.n do W[table.getn(W)+1] = tostring(arg[i]) end end
local function flush() print(table.concat(W, "")); W = {} end
-- numeric for
local s = ""
for i = 1, 5 do s = s .. i .. "," end
print(s)
for i = 10, 1, -3 do w(i, " ") end flush()
for i = 1, 2, 0.5 do w(i, " ") end flush()
for i = 1, "3" do w(i, " ") end flush()
for i = 1, 10 do if i == 4 then break end w(i, ";") end flush()
-- modifying the loop variable (5.0: it is the index itself)
local n = 0
for i = 1, 10 do i = i + 2; n = n + 1; w(i, " ") end flush() print("n", n)
-- closures in loops share the 5.0 loop variable
local fs = {}
for i = 1, 3 do fs[i] = function() return i end end
print(fs[1](), fs[2](), fs[3]())
-- generic for over a table (TFORPREP)
local t = {"gold","silver","copper"}
for i, m in t do w(i, "=", m, " ") end flush()
for i, m in ipairs(t) do w(i, "=", m, " ") end flush()
local cnt = 0
for k, v in pairs({a=1,b=2,c=3}) do cnt = cnt + v end print("sum", cnt)
-- SETLIST with many items, SETLISTO
local big = {1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70}
print(table.getn(big), big[1], big[33], big[64], big[70])
local function three() return 7, 8, 9 end
local o = {1, 2, three()}
print(table.getn(o), o[3], o[5])
local o2 = {three(), three()}
print(table.getn(o2), o2[1], o2[2], o2[4])
-- and/or, not, comparisons
local x = nil
print(x or "d", false and 1, 1 and 2, not nil, not 0, 1 < 2, "a" < "b", 2 <= 2, 3 >= 4, 1 == 1.0, "1" == 1)
-- arithmetic and coercion
print(1 + "2", "10" * 2, 7 / 2, math.mod(7, 3), math.floor(-3.5), -(-4), 2^10)
print(10 .. 20, 1.5 .. "", 1e15, 1e16, 0.1, 1/3)
print(string.format("%d %5.2f %s %q", 42, 3.14159, "x", "a\"b"))
print(string.find("on_click_zeppelin", "click"), string.sub("abcdef", 2, -2), string.len("abc"), string.rep("ab", 3))
print(tostring(nil), tostring(true), tonumber("0x10"), tonumber("  12  "), tonumber("z", 36), type(print))
-- upvalues, recursion, methods
local function counter() local c = 0 return function() c = c + 1 return c end end
local c1 = counter(); c1(); print(c1())
local function fib(k) if k < 2 then return k end return fib(k-1) + fib(k-2) end
print(fib(15))
local obj = {v = 5}
function obj:get(a) return self.v + a end
print(obj:get(3))
-- while / repeat
local w = 0 while w < 5 do w = w + 1 end
repeat w = w - 2 until w < 0
print(w)
-- nested tables and globals
G = {a = {b = {c = "deep"}}}
print(G.a.b.c, G["a"]["b"].c)
-- varargs in 5.0: arg
local function va(...) return arg.n, arg[1], arg[2] end
print(va(4, 5, 6))
-- error messages
print(pcall(function() local z = nil; return z.x end))
print(pcall(function() return undefined_global_fn() end))
print(pcall(function() assert(false, "boom") end))
print(pcall(function() error("msg") end))
print(pcall(function() for i = "a", 2 do end end))

-- hash order probe (5.0 and 5.1 tables differ in layout rules)
local h = {}
for _, k in {"zeppelin","macao","ishtar","etage0","etage12","etage18","etage19","etage28","etage99","etage-2","mesopotamie"} do h[k] = true end
local order = {}
for k in pairs(h) do order[table.getn(order)+1] = k end
print(table.concat(order, " "))
local hn = {}
for i = 1, 20 do hn[i * 7.5] = i end
local o2 = {}
for k in pairs(hn) do o2[table.getn(o2)+1] = k end
print(table.concat(o2, " "))
