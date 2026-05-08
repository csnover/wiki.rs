do
  local res = ""
  for k, v in string.gmatch("|a|b|c|d|", "|([^|]*)|([^|]*)") do
    res = res .. k .. "=" .. v .. ";"
  end
  assert(res == "a=b;c=d;")
end
