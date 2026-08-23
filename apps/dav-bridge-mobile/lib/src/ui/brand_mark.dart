import 'package:flutter/material.dart';

class KamoriBrandMark extends StatelessWidget {
  const KamoriBrandMark({super.key, this.size = 28});

  final double size;

  @override
  Widget build(BuildContext context) => Semantics(
        label: 'Kamori',
        image: true,
        child: CustomPaint(
          size: Size.square(size),
          painter: const _KamoriBrandPainter(),
        ),
      );
}

class _KamoriBrandPainter extends CustomPainter {
  const _KamoriBrandPainter();

  @override
  void paint(Canvas canvas, Size size) {
    final scale = size.width / 48;
    canvas.scale(scale, scale);

    final page = Path()
      ..moveTo(8, 4)
      ..lineTo(30, 4)
      ..lineTo(40, 14)
      ..lineTo(40, 44)
      ..lineTo(8, 44)
      ..close();
    canvas.drawPath(page, Paint()..color = const Color(0xFF173F37));

    final fold = Path()
      ..moveTo(30, 4)
      ..lineTo(30, 14)
      ..lineTo(40, 14)
      ..close();
    canvas.drawPath(fold, Paint()..color = const Color(0xFFF1B95B));

    final lightStroke = Paint()
      ..color = const Color(0xFFF6F0E4)
      ..strokeWidth = 4.5
      ..strokeCap = StrokeCap.round;
    canvas.drawLine(const Offset(17, 14), const Offset(17, 35), lightStroke);
    canvas.drawLine(const Offset(18, 25), const Offset(29, 14), lightStroke);
    canvas.drawLine(
      const Offset(18, 25),
      const Offset(31, 37),
      Paint()
        ..color = const Color(0xFFE76F58)
        ..strokeWidth = 4.5
        ..strokeCap = StrokeCap.round,
    );
  }

  @override
  bool shouldRepaint(_KamoriBrandPainter oldDelegate) => false;
}

class KamoriAppBarTitle extends StatelessWidget {
  const KamoriAppBarTitle({super.key});

  @override
  Widget build(BuildContext context) => const Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          KamoriBrandMark(),
          SizedBox(width: 10),
          Text('Kamori'),
        ],
      );
}
