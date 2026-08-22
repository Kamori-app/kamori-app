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

    final outer = Path()
      ..moveTo(8, 8.5)
      ..lineTo(21.5, 8.5)
      ..cubicTo(31.7, 8.5, 40, 16.8, 40, 27)
      ..lineTo(40, 39.5)
      ..lineTo(26.5, 39.5)
      ..cubicTo(16.3, 39.5, 8, 31.2, 8, 21)
      ..close();
    canvas.drawPath(outer, Paint()..color = const Color(0xFF173F37));

    final inner = Path()
      ..moveTo(13.5, 13.5)
      ..lineTo(21.7, 13.5)
      ..cubicTo(29.1, 13.5, 35, 19.5, 35, 26.8)
      ..lineTo(35, 34.5)
      ..lineTo(26.8, 34.5)
      ..cubicTo(19.4, 34.5, 13.5, 28.5, 13.5, 21.2)
      ..close();
    canvas.drawPath(inner, Paint()..color = const Color(0xFFF1B95B));

    canvas.drawLine(
      const Offset(13.5, 34.5),
      const Offset(34.8, 13.2),
      Paint()
        ..color = const Color(0xFFF6F0E4)
        ..strokeWidth = 4.5,
    );
    canvas.drawLine(
      const Offset(23, 25),
      const Offset(35, 37),
      Paint()
        ..color = const Color(0xFFE76F58)
        ..strokeWidth = 4.5,
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
