#import "/templates/base.typ": base

#show: base

Linux 在启动时, 会创建一个 init 进程, 此时自动创建 3 个特殊的文件描述符, 对应 3 个设备IO文件: \

#table(
  columns: 3,
  stroke: gray,
  inset: 10pt,
  [文件->], [含义], [描述符],
  [/dev/stdin], [标准输入], [0],
  [/dev/stdout], [标准输出], [1],
  [/dev/stderr], [标准错误], [2]
)

