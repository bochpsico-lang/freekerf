// Generates dotnet_format.tsv: seeded values formatted by Mono (the runtime LaserGRBL's
// goldens were recorded with). Columns: bits of the double (hex), double.ToString(),
// ToString("0.###"), ToString("0.00"), ToString("0"), then the same value as float:
// float bits (hex), float.ToString(), float.ToString("0.###").
//
//   mcs -out:fmt.exe DotnetFormat.cs && mono fmt.exe > dotnet_format.tsv
using System;
using System.Globalization;

static class DotnetFormat
{
	static void Main()
	{
		var inv = CultureInfo.InvariantCulture;
		var rnd = new Random(42);
		for (int i = 0; i < 4000; i++)
		{
			double v;
			switch (i % 8)
			{
				case 0: v = rnd.NextDouble() * 1000; break;
				case 1: v = Math.Round(rnd.NextDouble() * 1000, rnd.Next(6)) + (rnd.Next(3) - 1) * 1e-13; break;
				case 2: v = (rnd.NextDouble() - 0.5) * Math.Pow(10, rnd.Next(-8, 20)); break;
				case 3: v = rnd.Next(-100000, 100000) / 1000.0 + 0.0005; break;
				case 4: v = BitConverter.Int64BitsToDouble((long)(rnd.NextDouble() * long.MaxValue)); break;
				case 5: v = (rnd.Next(2000) - 1000) / 8.0; break;
				case 6: v = 0.005 + rnd.Next(2000) / 1000.0 - rnd.Next(2000) / 1000.0; break;
				default: v = (float)(rnd.NextDouble() * 500) / 255.0; break;
			}
			if (double.IsNaN(v) || double.IsInfinity(v)) v = 1;
			float f = (float)v;
			Console.WriteLine(string.Join("\t", BitConverter.DoubleToInt64Bits(v).ToString("X16"), v.ToString(inv), v.ToString("0.###", inv),
				v.ToString("0.00", inv), v.ToString("0", inv), BitConverter.ToInt32(BitConverter.GetBytes(f), 0).ToString("X8"), f.ToString(inv), f.ToString("0.###", inv)));
		}
	}
}
