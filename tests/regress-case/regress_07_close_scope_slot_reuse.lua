-- regress_07_close_scope_slot_reuse#1: close 后复用寄存器不能写回已关闭 upvalue
local u
do
    local count = 0
    local function co()
        count = count + 1
        return count
    end
    u = co
end

print("regress_07_close_scope_slot_reuse#1", u(), u())

-- regress_07_close_scope_slot_reuse#2: 当前赋值 RHS 捕获同槽位时应写回原 local
local outer = 1
do
    local inner = 2
    inner = function()
        return inner
    end
end

print("regress_07_close_scope_slot_reuse#2", outer)