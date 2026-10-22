-- Defines `close()`: ends vista's side-by-side diff tab page, if any,
-- without reporting it (the caller already knows).
local ALIGN_NS = vim.api.nvim_create_namespace('vista_diff_align')

local function close()
  pcall(vim.api.nvim_del_augroup_by_name, 'vista_diff')
  for _, tp in ipairs(vim.api.nvim_list_tabpages()) do
    local ok, v = pcall(vim.api.nvim_tabpage_get_var, tp, 'vista_diff')
    local okb, b = pcall(vim.api.nvim_tabpage_get_var, tp, 'vista_diff_buf')
    if okb and vim.api.nvim_buf_is_valid(b) then
      for _, k in ipairs({ 'zi', 'zn', 'zN' }) do
        pcall(vim.keymap.del, 'n', k, { buffer = b })
      end
      vim.api.nvim_buf_clear_namespace(b, ALIGN_NS, 0, -1)
    end
    if ok and v then
      vim.api.nvim_set_current_tabpage(tp)
      vim.cmd('diffoff!')
      pcall(vim.cmd, 'tabclose')
    end
  end
  if vim.g.vista_stal ~= nil then
    vim.o.showtabline = vim.g.vista_stal
    vim.g.vista_stal = nil
  end
  if vim.g.vista_dip ~= nil then
    vim.o.diffopt = vim.g.vista_dip
    vim.g.vista_dip = nil
  end
end
