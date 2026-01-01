#import "/templates/base.typ": base

#show: base

因此, 尽管嵌入式计算机的数量极其庞大, 还是有很多用户没有意识到他们在使用计算机 \

#figure[
#table(columns: 7, inset: 10pt,
  [*十进制术语*], [*缩写*], [*数值*], [*二进制术语*], [*缩写*], [*数值*], [*数值差别*],
  [kilobyte], [KB], [$1000^1$], [kibibyte], [KiB], [$2^10$], [2%],
  [megabyte], [MB], [$1000^2$], [mebibyte], [MiB], [$2^20$], [5%],
  [gigabyte], [GB], [$1000^3$], [gibibyte], [GiB], [$2^30$], [7%],
  [terabyte], [TB], [$1000^4$], [tebibyte], [TiB], [$2^40$], [10%],
  [petabyte], [PB], [$1000^5$], [pebibyte], [PiB], [$2^50$], [13%],
  [exabyte], [EB], [$1000^6$], [exbibyte], [EiB], [$2^60$], [15%],
  [zettabyte], [ZB], [$1000^7$], [zebibyte], [ZiB], [$2^70$], [18%],
  [yottabyte], [YB], [$1000^8$], [yobibyte], [YiB], [$2^80$], [21%],
  [ronnayte], [RB], [$1000^9$], [robibyte], [RiB], [$2^90$], [24%],
  [queccabyte], [QB], [$1000^10$], [quebibyte], [QiB], [$2^100$], [27%],
)]

上图通过为常用容量加一个二进制标记, 以解决 $2^x$ 与 $10^y$ 字节的模糊性 \
最后一列表示二进制术语与相应的十进制术语所表示数值之间的差距 \
在以 bit 为单位时, 这些表示方法同样适用, 因此 gigabit(Gb) 是 $10^9$ bit, 而gibibit(Gib) 是 $2^30$ bit \
使用公制系统的组织创建了十进制前缀, 而为适应存储系统总容量的不断增加

