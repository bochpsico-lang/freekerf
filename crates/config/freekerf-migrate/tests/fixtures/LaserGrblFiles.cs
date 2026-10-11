// Writes LaserGRBL data files with LaserGRBL's own types and serializers, as fixtures
// for freekerf-migrate (read-only importer). Build and run with Mono:
//   mcs -r:LaserGRBL.exe -r:System.Drawing.dll LaserGrblFiles.cs && mono LaserGrblFiles.exe <out dir>
// (LaserGRBL.exe from a LaserGRBL build; the conformance build works.)
using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Runtime.Serialization.Formatters.Binary;

class LaserGrblFiles
{
	static object Enum(Assembly a, string type, string value)
	{
		return System.Enum.Parse(a.GetType(type, true), value);
	}

	static void Main(string[] args)
	{
		string outDir = args.Length > 0 ? args[0] : ".";
		Assembly lg = typeof(LaserGRBL.GrblCore).Assembly;

		// LaserGRBL.Settings.bin: Settings.InternalSaveFile serializes this dictionary
		var dic = new Dictionary<string, object>();
		dic["Current LaserGRBL Version"] = new Version(7, 13, 0);
		dic["Firmware Type"] = LaserGRBL.Firmware.Smoothie;
		dic["Serial Speed"] = 250000;
		dic["Streaming Mode"] = Enum(lg, "LaserGRBL.GrblCore+StreamingMode", "RepeatOnError");
		dic["Reset Grbl On Connect"] = false;
		dic["Query MachineInfo ($I) at connect"] = false;
		dic["Support Hardware PWM"] = false;
		dic["Enable Continuous Jog"] = true;
		dic["AutoCooling"] = true;
		dic["AutoCooling TOn"] = TimeSpan.FromMinutes(7);
		dic["AutoCooling TOff"] = TimeSpan.FromSeconds(90);
		dic["GCode.CustomHeader"] = "G90 (absolute)\r\nM3 S[0]";
		dic["GCode.CustomFooter"] = "G0 X0 Y0 Z0\r\nM5";
		dic["GCode.CustomPasses"] = ";pass\r\nG4 P1";
		dic["Jog Step"] = 2.5m;
		dic["Jog Speed"] = 1500;
		dic["Disable G0 fast skip"] = true;
		dic["Unidirectional Engraving"] = true;
		dic["GrayScaleConversion.Gcode.Speed.Mark"] = 2000;
		dic["GrayScaleConversion.Line2LineOptions.Quality"] = 7.5m;
		dic["GrayScaleConversion.Line2LineOptions.Direction"] = Enum(lg, "LaserGRBL.RasterConverter.ImageProcessor+Direction", "Diagonal");
		dic["GrayScaleConversion.Parameters.Interpolation"] = System.Drawing.Drawing2D.InterpolationMode.HighQualityBicubic;
		dic["GrayScaleConversion.Gcode.Offset.X"] = 1.5F;
		dic["GrayScaleConversion.Gcode.ImageSize.W"] = 100F;
		dic["Last GrblVersion known"] = null;
		dic["Color Schema"] = Enum(lg, "LaserGRBL.ColorScheme+Scheme", "CADDark");
		dic["Mainform Size and Position"] = new System.Drawing.Rectangle(10, 20, 800, 600);
		dic["Ratio"] = 0.25;
		dic["Big"] = 1234567890123L;
		dic["Letter"] = 'x';
		dic["Same string twice A"] = "shared";
		dic["Same string twice B"] = "shared";
		var f = new BinaryFormatter();
		f.AssemblyFormat = System.Runtime.Serialization.Formatters.FormatterAssemblyStyle.Simple;
		using (var fs = File.Create(Path.Combine(outDir, "LaserGRBL.Settings.bin")))
			f.Serialize(fs, dic);

		// LaserLifeCounter.bin: LaserLifeHandler saves a ListLLC with Serializer.ObjToFile
		Type handler = lg.GetType("LaserGRBL.LaserLifeHandler", true);
		Type listType = handler.GetNestedType("ListLLC");
		Type counterType = handler.GetNestedType("LaserLifeCounter");
		var list = (System.Collections.IList)Activator.CreateInstance(listType);
		object def = counterType.GetMethod("CreateDefault").Invoke(null, null);
		BindingFlags pf = BindingFlags.NonPublic | BindingFlags.Instance;
		Action<object, string, object> set = (o, n, v) => counterType.GetField(n, pf).SetValue(o, v);
		set(def, "mGuid", "0b1f6f2e-6f7c-4c1e-9f55-1d2c3b4a5e6f");
		set(def, "mMonitoringDate", new DateTime(2023, 3, 4));
		set(def, "mLastUsage", new DateTime(2024, 5, 6));
		set(def, "mTimeInRun", TimeSpan.FromHours(12.5));
		set(def, "mTimeUsageNormalizedPower", TimeSpan.FromHours(4));
		set(def, "mTimeUsageNonZero", TimeSpan.FromHours(6));
		var classes = new TimeSpan[10];
		for (int i = 0; i < 10; i++) classes[i] = TimeSpan.FromMinutes(i * 3);
		set(def, "mTimeClasses", classes);
		list.Add(def);
		object second = counterType.GetMethod("CreateDefault").Invoke(null, null);
		set(second, "mGuid", "a2c4e6f8-0000-4000-8000-123456789abc");
		set(second, "mName", "Diode 10 W");
		set(second, "mBrand", "Acme");
		set(second, "mModuleModel", "LD-10");
		set(second, "mOpticalPower", (double?)10.0);
		set(second, "mPurchaseDate", (DateTime?)new DateTime(2022, 1, 2));
		set(second, "mMonitoringDate", (DateTime?)new DateTime(2022, 1, 3));
		set(second, "mDeathDate", (DateTime?)new DateTime(2024, 12, 31));
		set(second, "mTimeInRun", TimeSpan.FromMinutes(95));
		list.Add(second);
		listType.GetField("LastSent").SetValue(list, new DateTime(2024, 1, 1));
		Type ser = lg.GetType("Tools.Serializer", false) ?? lg.GetType("LaserGRBL.Tools.Serializer", true);
		MethodInfo save = ser.GetMethod("ObjToFile", new[] { typeof(object), typeof(string) });
		string llc = Path.GetFullPath(Path.Combine(outDir, "LaserLifeCounter.bin"));
		if (!(bool)save.Invoke(null, new object[] { list, llc }))
			throw new Exception("ObjToFile failed");

		// CustomButtons.bin (CustomButtons.SaveFile) and an exported .zbn
		// (CustomButtons.Export: binary, compressed): every button type and style
		var buttons = new List<LaserGRBL.CustomButton>();
		var icon = new System.Drawing.Bitmap(4, 3);
		icon.SetPixel(1, 1, System.Drawing.Color.Red);
		buttons.Add(new LaserGRBL.CustomButton {
			guid = new Guid("11111111-2222-3333-4444-555555555555"),
			Caption = "Frame", ToolTip = "Frame the job (3%)", Image = icon,
			GCode = "G0 X[left] Y[bottom]\r\nM3 S[$30*3/100] F1000\r\nG1 Y[top]\r\nM5",
			GCode2 = "",
			EnableStyle = LaserGRBL.CustomButton.EnableStyles.IdleProgram,
			ButtonType = LaserGRBL.CustomButton.ButtonTypes.Button });
		buttons.Add(new LaserGRBL.CustomButton {
			Caption = "Air", ToolTip = "Air assist \"on\"/off \\ é",
			GCode = "M8", GCode2 = "M9",
			EnableStyle = LaserGRBL.CustomButton.EnableStyles.Connected,
			ButtonType = LaserGRBL.CustomButton.ButtonTypes.TwoStateButton });
		buttons.Add(new LaserGRBL.CustomButton {
			Caption = "", ToolTip = "Hold to fire",
			GCode = "M3 S[$30/100]\r\nG1 F100", GCode2 = "M5",
			EnableStyle = LaserGRBL.CustomButton.EnableStyles.Idle,
			ButtonType = LaserGRBL.CustomButton.ButtonTypes.PushButton });
		buttons.Add(new LaserGRBL.CustomButton {
			Caption = "Pause", ToolTip = "",
			GCode = "!", GCode2 = null,
			EnableStyle = LaserGRBL.CustomButton.EnableStyles.Run,
			ButtonType = LaserGRBL.CustomButton.ButtonTypes.Button });
		buttons.Add(new LaserGRBL.CustomButton {
			Caption = "Home", ToolTip = "Home",
			GCode = "$H", GCode2 = null,
			EnableStyle = LaserGRBL.CustomButton.EnableStyles.Always,
			ButtonType = LaserGRBL.CustomButton.ButtonTypes.Button });
		string cbFile = Path.GetFullPath(Path.Combine(outDir, "CustomButtons.bin"));
		if (!(bool)save.Invoke(null, new object[] { buttons, cbFile }))
			throw new Exception("ObjToFile failed");
		Type modeType = ser.GetNestedType("SerializationMode");
		MethodInfo saveFull = ser.GetMethod("ObjToFile", new[] { typeof(object), typeof(string), modeType, typeof(string), typeof(bool) });
		string zbn = Path.GetFullPath(Path.Combine(outDir, "CustomButtons.zbn"));
		if (!(bool)saveFull.Invoke(null, new object[] { buttons, zbn, System.Enum.Parse(modeType, "Binary"), null, true }))
			throw new Exception("ObjToFile (compressed) failed");
		Console.WriteLine("ok");
	}
}
