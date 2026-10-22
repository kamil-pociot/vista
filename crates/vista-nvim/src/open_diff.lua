-- Opens a file side by side with its HEAD version in diff mode, on a tab
-- page of its own (follows close_diff.lua, which defines `close()` and
-- ALIGN_NS). Arguments: path, HEAD text, HEAD buffer name, cursor line,
-- top line (working file, or HEAD when only HEAD has the top row), left
-- column, wrap, collapse.
local p, head, label, l, top, top_head, leftcol, wrap, collapse = ...
-- Missing values arrive as vim.NIL, which is truthy.
local function given(v)
  if v == vim.NIL then return nil end
  return v
end
l, top, top_head, leftcol = given(l), given(top), given(top_head), given(leftcol)
close()
vim.g.vista_stal = vim.o.showtabline
vim.o.showtabline = 0
-- vista aligns the sides itself (below), wrapped lines included; Neovim's
-- filler can't account for wrapping, so it's off while the diff is open.
vim.g.vista_dip = vim.o.diffopt
vim.opt.diffopt:remove('filler')
-- linematch adds Neovim's filler regardless; vista's pairing still uses it
-- (from the saved diffopt).
vim.o.diffopt = vim.o.diffopt:gsub(',?linematch:%d+', '')

vim.cmd.tabedit(vim.fn.fnameescape(p))
vim.t.vista_diff = true
local work = vim.api.nvim_get_current_win()
local work_buf = vim.api.nvim_win_get_buf(work)
vim.t.vista_diff_buf = work_buf
vim.cmd('leftabove vnew')
local head_win = vim.api.nvim_get_current_win()
local buf = vim.api.nvim_get_current_buf()
local lines = vim.split(head, '\n', { plain = true })
if lines[#lines] == '' then table.remove(lines) end
vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
vim.bo[buf].buftype = 'nofile'
vim.bo[buf].bufhidden = 'wipe'
vim.bo[buf].swapfile = false
vim.bo[buf].modifiable = false
vim.bo[buf].readonly = true
pcall(vim.api.nvim_buf_set_name, buf, label)
local ft = vim.filetype.match({ filename = p })
if ft then vim.bo[buf].filetype = ft end
-- Lines only in HEAD read as deleted (IntelliJ), not as added.
vim.wo[head_win].winhighlight = 'DiffAdd:VistaDiffDeleted'
vim.cmd('diffthis')
vim.api.nvim_set_current_win(work)
vim.cmd('diffthis')
-- Diff mode turns wrapping off and folds unchanged lines; follow the
-- viewer's wrap and collapse settings instead. Line numbers on both sides.
for _, w in ipairs({ head_win, work }) do
  vim.wo[w].wrap = wrap
  vim.wo[w].foldenable = collapse
  vim.wo[w].number = true
end

-- Neovim's scroll binding maps lines through its own diff and can't count
-- vista's padding rows; the sides are kept in step below instead.
for _, w in ipairs({ head_win, work }) do
  vim.wo[w].scrollbind = false
  vim.wo[w].cursorbind = false
end

-- Alignment: pair the lines like diff mode does (same algorithm and
-- linematch), then give the shorter side of every pair blank rows, and the
-- other side ╱ rows for lines it doesn't have, like vista's own diff. Padding
-- sits *above* the next line, where `topfill` can scroll into it.
local function diff_opts()
  local o = { result_type = 'indices' }
  for _, part in ipairs(vim.split(vim.g.vista_dip or vim.o.diffopt, ',')) do
    local k, v = part:match('^(%w+):(.+)$')
    if k == 'algorithm' then o.algorithm = v end
    if k == 'linematch' then o.linematch = tonumber(v) end
  end
  return o
end

-- Screen rows of 1-based line `lnum` in `win` (closed folds: 1 for the
-- fold's first line, 0 for the rest), padding not included.
local function rows(win, lnum)
  local fc = vim.api.nvim_win_call(win, function() return vim.fn.foldclosed(lnum) end)
  if fc ~= -1 then return fc == lnum and 1 or 0 end
  local t = vim.api.nvim_win_text_height(win, { start_row = lnum - 1, end_row = lnum - 1 })
  return t.all - t.fill
end

-- Per window: start[l] = first aligned row of line l's padding, pad[l] =
-- padding rows above line l, h[l] = rows of line l.
local layout = {}

local function realign()
  if not (vim.api.nvim_win_is_valid(work) and vim.api.nvim_win_is_valid(head_win)) then return end
  for _, b in ipairs({ buf, work_buf }) do
    vim.api.nvim_buf_clear_namespace(b, ALIGN_NS, 0, -1)
  end
  local a = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  local b = vim.api.nvim_buf_get_lines(work_buf, 0, -1, false)
  local hunks = vim.diff(table.concat(a, '\n') .. '\n', table.concat(b, '\n') .. '\n', diff_opts())
  local side = {
    [buf] = { win = head_win, n = #a, pads = {} },
    [work_buf] = { win = work, n = #b, pads = {} },
  }
  -- Padding rows above line `before` (1-based; n + 1 = after the last line).
  local function add(target, before, count, filler)
    if count <= 0 then return end
    local sd = side[target]
    local list = sd.pads[before] or {}
    local text = filler and string.rep('╱', vim.api.nvim_win_get_width(sd.win)) or ''
    for _ = 1, count do
      table.insert(list, { { text, filler and 'DiffDelete' or 'Normal' } })
    end
    sd.pads[before] = list
  end
  local function pair(i, j)
    local ha, hb = rows(head_win, i), rows(work, j)
    if ha > hb then add(work_buf, j + 1, ha - hb, false) end
    if hb > ha then add(buf, i + 1, hb - ha, false) end
  end
  local ia, ib = 1, 1
  for _, hk in ipairs(hunks) do
    local sa, ca, sb, cb = hk[1], hk[2], hk[3], hk[4]
    -- With a count of 0, the start is the line *before* the hunk.
    local last_same = ca == 0 and sa or sa - 1
    while ia <= last_same do
      pair(ia, ib)
      ia, ib = ia + 1, ib + 1
    end
    local n = math.min(ca, cb)
    for k = 0, n - 1 do pair(ia + k, ib + k) end
    if ca > cb then
      local fill = 0
      for k = n, ca - 1 do fill = fill + rows(head_win, ia + k) end
      add(work_buf, ib + n, fill, true)
    elseif cb > ca then
      local fill = 0
      for k = n, cb - 1 do fill = fill + rows(work, ib + k) end
      add(buf, ia + n, fill, true)
    end
    ia, ib = ia + ca, ib + cb
  end
  while ia <= #a and ib <= #b do
    pair(ia, ib)
    ia, ib = ia + 1, ib + 1
  end
  layout = {}
  for target, sd in pairs(side) do
    for before, vl in pairs(sd.pads) do
      if before <= sd.n then
        vim.api.nvim_buf_set_extmark(target, ALIGN_NS, before - 1, 0, { virt_lines = vl, virt_lines_above = true })
      elseif sd.n > 0 then
        vim.api.nvim_buf_set_extmark(target, ALIGN_NS, sd.n - 1, 0, { virt_lines = vl })
      end
    end
    local L = { start = {}, pad = {}, h = {}, n = sd.n }
    local row = 0
    for l = 1, sd.n do
      L.start[l] = row
      L.pad[l] = sd.pads[l] and #sd.pads[l] or 0
      L.h[l] = rows(sd.win, l)
      row = row + L.pad[l] + L.h[l]
    end
    layout[sd.win] = L
  end
end

-- Keeps the other side at the same aligned row (and column) as `from`.
local programmatic = {}
local function sync(from)
  local to = from == work and head_win or work
  local F, T = layout[from], layout[to]
  if not (F and T and vim.api.nvim_win_is_valid(to)) or F.n == 0 or T.n == 0 then return end
  local v = vim.api.nvim_win_call(from, vim.fn.winsaveview)
  local top = math.min(v.topline, F.n)
  local row = F.start[top] + math.max(F.pad[top] - (v.topfill or 0), 0)
  -- The other side's line holding that row (binary search).
  local lo, hi = 1, T.n
  while lo < hi do
    local mid = math.floor((lo + hi + 1) / 2)
    if T.start[mid] <= row then lo = mid else hi = mid - 1 end
  end
  local into = row - T.start[lo]
  local topfill = into < T.pad[lo] and (T.pad[lo] - into) or 0
  local want = { topline = lo, topfill = topfill, leftcol = v.leftcol }
  local cur = vim.api.nvim_win_call(to, vim.fn.winsaveview)
  if cur.topline == want.topline and (cur.topfill or 0) == topfill and cur.leftcol == v.leftcol then return end
  programmatic[to] = true
  -- Keep that side's cursor on a line fully in view (counting wrapped rows
  -- and padding), or Neovim scrolls back to it.
  local height = vim.api.nvim_win_get_height(to)
  local last, used = lo, topfill + T.h[lo]
  while last < T.n and used + T.pad[last + 1] + T.h[last + 1] <= height do
    last = last + 1
    used = used + T.pad[last] + T.h[last]
  end
  local lnum = math.max(lo, math.min(cur.lnum, last))
  vim.api.nvim_win_call(to, function()
    vim.fn.winrestview({ topline = lo, topfill = topfill, leftcol = v.leftcol, lnum = lnum })
  end)
end

-- Realign at most once per tick, however many events ask for it.
local pending = false
local function schedule_realign()
  if pending then return end
  pending = true
  vim.schedule(function()
    pending = false
    realign()
    sync(vim.api.nvim_get_current_win() == head_win and head_win or work)
  end)
end

realign()
-- Open where vista's diff was: same top line, cursor line and column.
vim.api.nvim_win_call(work, function()
  vim.fn.winrestview({ lnum = l or 1, col = 0, topline = top or l or 1, leftcol = leftcol or 0 })
end)
if top_head then
  vim.api.nvim_win_call(head_win, function()
    vim.fn.winrestview({ lnum = top_head, topline = top_head, leftcol = leftcol or 0 })
  end)
  sync(head_win)
else
  sync(work)
end

local g = vim.api.nvim_create_augroup('vista_diff', { clear = true })
vim.api.nvim_create_autocmd({ 'TextChanged', 'TextChangedI' }, {
  group = g,
  buffer = work_buf,
  callback = schedule_realign,
})
vim.api.nvim_create_autocmd({ 'WinResized', 'VimResized' }, { group = g, callback = schedule_realign })
-- Scrolling either side (wheel, keys, cursor) moves the other one along.
vim.api.nvim_create_autocmd('WinScrolled', {
  group = g,
  callback = function()
    for _, w in ipairs({ work, head_win }) do
      local e = vim.v.event[tostring(w)]
      if e and (e.topline ~= 0 or e.leftcol ~= 0 or e.topfill ~= 0) then
        if programmatic[w] then
          programmatic[w] = nil
        else
          sync(w)
        end
      end
    end
  end,
})
-- `:set wrap` on one side applies to both, so they stay aligned.
vim.api.nvim_create_autocmd('OptionSet', {
  group = g,
  pattern = { 'wrap', 'number', 'relativenumber', 'linebreak' },
  callback = function(ev)
    if ev.match == 'wrap' then
      local v = vim.wo.wrap
      for _, w in ipairs({ head_win, work }) do
        if vim.api.nvim_win_is_valid(w) then vim.wo[w].wrap = v end
      end
    end
    schedule_realign()
  end,
})
-- `zi`/`zn`/`zN` don't fire OptionSet: map them (diff buffers only) so they
-- also reach the other side and vista.
local function set_folding(v)
  for _, w in ipairs({ head_win, work }) do
    if vim.api.nvim_win_is_valid(w) then vim.wo[w].foldenable = v end
  end
  vim.rpcnotify(0, 'vista_collapse', v)
  schedule_realign()
end
for _, bb in ipairs({ buf, work_buf }) do
  for _, k in ipairs({ 'zi', 'zn', 'zN' }) do
    vim.keymap.set('n', k, function()
      vim.cmd('normal! ' .. k)
      set_folding(vim.wo.foldenable)
    end, { buffer = bb, desc = 'vista: fold unchanged lines' })
  end
end
-- `:set foldenable` on one side applies to both, and to vista's collapse.
vim.api.nvim_create_autocmd('OptionSet', {
  group = g,
  pattern = 'foldenable',
  callback = function() set_folding(vim.wo.foldenable) end,
})
vim.api.nvim_create_autocmd('WinClosed', {
  group = g,
  pattern = { tostring(work), tostring(head_win) },
  callback = function()
    vim.schedule(function()
      close()
      vim.rpcnotify(0, 'vista_diff_closed')
    end)
  end,
})
