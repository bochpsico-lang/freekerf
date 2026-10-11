// Generates clipper_cases.jsonl: seeded random inputs and what LaserGRBL's
// Clipper 6.4.2 (LaserGRBL/Clipper/Clipper.cs) returns for them. FreeKerf's port
// must reproduce every output exactly (tests/clipper_differential.rs).
//
//   mcs -out:cases.exe ClipperCases.cs $LASERGRBL_REPO/LaserGRBL/Clipper/Clipper.cs
//   mono cases.exe > clipper_cases.jsonl
using System;
using System.Collections.Generic;
using System.Text;
using ClipperLib;

static class ClipperCases
{
	static Random rnd = new Random(20261007);

	static List<IntPoint> RandomPath(int n, int range, bool grid)
	{
		var p = new List<IntPoint>();
		for (int i = 0; i < n; i++)
		{
			long x = rnd.Next(range), y = rnd.Next(range);
			if (grid) { x -= x % 10; y -= y % 10; }
			p.Add(new IntPoint(x, y));
		}
		return p;
	}

	static string P(List<IntPoint> p)
	{
		var sb = new StringBuilder("[");
		for (int i = 0; i < p.Count; i++)
			sb.Append(i > 0 ? "," : "").Append("[").Append(p[i].X).Append(",").Append(p[i].Y).Append("]");
		return sb.Append("]").ToString();
	}

	static string PP(List<List<IntPoint>> pp)
	{
		var sb = new StringBuilder("[");
		for (int i = 0; i < pp.Count; i++)
			sb.Append(i > 0 ? "," : "").Append(P(pp[i]));
		return sb.Append("]").ToString();
	}

	static void Main()
	{
		ClipType[] cts = { ClipType.ctIntersection, ClipType.ctUnion, ClipType.ctDifference, ClipType.ctXor };
		PolyFillType[] fts = { PolyFillType.pftEvenOdd, PolyFillType.pftNonZero, PolyFillType.pftPositive, PolyFillType.pftNegative };
		for (int k = 0; k < 440; k++)
		{
			int kind = k < 400 ? k % 4 : (k % 2 == 0 ? 0 : 1);
			bool grid = (k / 4) % 2 == 0;
			int range = (k / 8) % 3 == 0 ? 100 : 2000;
			var subj = new List<List<IntPoint>>();
			var clip = new List<List<IntPoint>>();
			if (kind == 0)
			{
				// LaserGRBL's use: open hatch lines clipped by closed shapes (even-odd), PolyTree
				// dense cases (k >= 400): many crossings in the same scanbeam (more than the 16 elements of the insertion sort)
				for (int i = 0; i < (k >= 400 ? 150 : 2 + rnd.Next(30)); i++) subj.Add(RandomPath(2 + (rnd.Next(4) == 0 ? rnd.Next(4) : 0), range, grid));
				for (int i = 0; i < 1 + rnd.Next(3); i++) clip.Add(RandomPath(3 + rnd.Next(8), range, grid));
				var c = new Clipper();
				c.AddPaths(subj, PolyType.ptSubject, false);
				c.AddPaths(clip, PolyType.ptClip, true);
				var tree = new PolyTree();
				bool ok = c.Execute(ClipType.ctIntersection, tree, PolyFillType.pftEvenOdd, PolyFillType.pftEvenOdd);
				Console.WriteLine("{\"kind\":\"open\",\"subj\":" + PP(subj) + ",\"clip\":" + PP(clip) + ",\"ok\":" + (ok ? "true" : "false")
					+ ",\"open\":" + PP(Clipper.OpenPathsFromPolyTree(tree)) + ",\"closed\":" + PP(Clipper.ClosedPathsFromPolyTree(tree)) + "}");
			}
			else if (kind == 1 || kind == 2)
			{
				for (int i = 0; i < 1 + rnd.Next(3); i++) subj.Add(RandomPath(k >= 400 ? 60 : 3 + rnd.Next(8), range, grid));
				for (int i = 0; i < rnd.Next(3); i++) clip.Add(RandomPath(k >= 400 ? 60 : 3 + rnd.Next(8), range, grid));
				ClipType ct = cts[rnd.Next(4)];
				PolyFillType sf = fts[rnd.Next(4)], cf = fts[rnd.Next(4)];
				var c = new Clipper();
				c.StrictlySimple = kind == 2 && rnd.Next(2) == 0;
				c.ReverseSolution = rnd.Next(4) == 0;
				c.AddPaths(subj, PolyType.ptSubject, true);
				c.AddPaths(clip, PolyType.ptClip, true);
				var sol = new List<List<IntPoint>>();
				bool ok;
				string closed;
				if (kind == 2)
				{
					var tree = new PolyTree();
					ok = c.Execute(ct, tree, sf, cf);
					closed = PP(Clipper.ClosedPathsFromPolyTree(tree));
				}
				else
				{
					ok = c.Execute(ct, sol, sf, cf);
					closed = PP(sol);
				}
				Console.WriteLine("{\"kind\":\"" + (kind == 2 ? "tree" : "paths") + "\",\"subj\":" + PP(subj) + ",\"clip\":" + PP(clip)
					+ ",\"ct\":" + (int)ct + ",\"sf\":" + (int)sf + ",\"cf\":" + (int)cf + ",\"simple\":" + (c.StrictlySimple ? "true" : "false")
					+ ",\"reverse\":" + (c.ReverseSolution ? "true" : "false") + ",\"ok\":" + (ok ? "true" : "false") + ",\"closed\":" + closed + "}");
			}
			else
			{
				for (int i = 0; i < 1 + rnd.Next(3); i++) subj.Add(RandomPath(3 + rnd.Next(8), range, grid));
				double delta = (rnd.NextDouble() - 0.6) * range / 5.0;
				var o = new ClipperOffset();
				o.AddPaths(subj, JoinType.jtRound, EndType.etClosedPolygon);
				var sol = new List<List<IntPoint>>();
				o.Execute(ref sol, delta);
				var sol2 = new List<List<IntPoint>>();
				o.Execute(ref sol2, delta * 2);
				Console.WriteLine("{\"kind\":\"offset\",\"subj\":" + PP(subj) + ",\"delta\":" + delta.ToString("R", System.Globalization.CultureInfo.InvariantCulture)
					+ ",\"closed\":" + PP(sol) + ",\"closed2\":" + PP(sol2) + "}");
			}
		}
	}
}
